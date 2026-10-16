# TH08 (東方永夜抄 ～ Imperishable Night. **ver 1.00d**) — Phase 1: engine architecture overview

*Sub-agent deliverable `sub/overview/00_overview.md` for the TH08 dirty-room RE effort.*
*Binary analysed:* `th08.exe`, PE32 (i386), image base `0x400000`, SHA256 `330fbdbf58a710829d65277b4f312cfbb38d5448b3df523e79350b879213d924`.
*Companion files:* `NOTES.md` (work log, open questions), `symbols_draft.csv` (proposed symbols for parent merge).

**Tagging convention.** Every claim below ends with **[V]** (VERIFIED: read directly from the disassembly/decompilation of the shipped binary, or from an external ABI reference and cross-checked at the call site) or **[I]** (INFERRED: consistent with surrounding evidence, not yet proven). Offsets are file VAs (i.e. as loaded at base `0x400000`). "vtbl+0xNN" means a COM vtable slot at that byte offset; the D3D8 slot map was cross-checked against Wine's `d3d8.h` and against the argument count at every call site (see §6.2). No decompiled code is reproduced in this document; the short pseudocode blocks are paraphrases.

---

## 1. Startup

### 1.1 Entry and top-level flow

| # | Step | Address | Evidence |
|---|------|---------|----------|
| 1 | CRT startup `entry` → calls WinMain | entry `0x4a619e` → `0x4418c0` | **[V]** |
| 2 | `WinMain(hInstance, …, cmdline, showCmd)` stores hInstance into `g_Supervisor+0x00` | `0x4418c0` | **[V]** |
| 3 | Save+disable screen saver / low-power mode via `SystemParametersInfoA` (0x10/0x53/0x54 saved into `0x17ce720/4/8`; 0x11/0x55/0x56 set to 0 with `SPIF_SENDWININICHANGE`) | `0x4418c0` | **[V]** |
| 4 | `Supervisor_InitLocks` — 4 × `InitializeCriticalSection` at `g_Supervisor+0x298`, stride `0x18` (slots: chain locks + logging/serialisation locks; chain adds/runs use index 0) | `0x4489f9` | **[V]** |
| 5 | `Logger_Print(&g_Logger, 0x4b5b94)` — writes the session banner "東方動作記録 ----…" into the rolling log buffer | `0x43ea00` | **[V]** |
| 6 | `Instance_Check` — `CreateMutexA(0, TRUE, "Touhou 08 App")`; if `GetLastError()==ERROR_ALREADY_EXISTS (0xB7)` → fatal log + return −1 (WinMain exits) | `0x443420` | **[V]** |
| 7 | `Config_Load("th08.cfg")` — reads the 60-byte config record through table loader `0x43e660(1)`, validates, applies, saves back if repaired (§1.5) | `0x4477b1` | **[V]** |
| 8 | `QueryPerformanceFrequency(&g_QPF at 0x17ce714)` | `0x4418c0` | **[V]** |
| 9 | Boot block (restartable, inside `do{…}while(true)`): D3D8 → window → device → sound → joystick → keyboard → AnmManager → boot scene (table below) | `0x4418c0` | **[V]** |
| 10 | Main loop `while (g_QuitFlag (0x17ce704) == 0)` | `0x4418c0` | **[V]** |

### 1.2 The restartable boot block

| Sub-step | Function | Notes |
|---|---|---|
| Direct3D 8 object | `0x442200` | `Direct3DCreate8(0xDC)` (SDK version 220), stored at `g_pD3D8` (0x17ce75c) **[V]** |
| Window | `0x442240` | Class `"BASE"` @0x4b5c60 (WndProc `0x442390`), title @0x4b5c30 = `"東方永夜抄　～ Imperishable Night. ver 1.00d"`; if `Supervisor_IsFullscreen()==0` → 640×480, style `0xCF0000` (`WS_OVERLAPPEDWINDOW`); sets `0x17ce708=1`,`0x17ce70c=0`; fullscreen branch uses style `0x100A0000` and sizes the window by the frame metrics **[V]** (style/size split between branches **[I]**) |
| D3D device | `0x4424c0` | See §1.3 **[V]** |
| Sound | `0x45c190(hwnd)` | `g_Bgm` (0x18b8a68): zeroes ~0x5224 bytes, creates DirectSound8 device 44100 Hz/16-bit/2ch, `DSBUFFERDESC` with flags `0x8008`, `SetTimer(hwnd, 0, 250 ms)` for streaming refill **[V]** |
| Joystick | `0x43d0c0` | `joyGetDevCapsA`/`joyGetPosEx`; state struct at `0x164d388` **[V]** |
| Keyboard state | `0x43e160` | keyboard state init (pairs with the DirectInput object created in 0x4464c8 per §5.3) **[V]** |
| AnmManager | `new(0x2a2570)` + `0x465070` ctor, tagged alloc `0x429de0(…, "SprtCtrlInf")` → `g_AnmManager` (0x18bdc90) | ~2.6 MB sprite/animation master object **[V]** |
| Window apply | `0x443ad0` | reads `Supervisor+0x13A` (see §7); when windowed also turns IME off and hides the cursor **[V]** |
| Boot scene | `0x445ea7` | returns 0 = ok, −1 = fatal exit, other = restart **[V]** |

Main-state counters initialised before the loop: `0x17ce710 = 0xFC` (frame counter seed), `0x17ce72c..0x17ce740` (now/prev/deadline doubles) = 0 **[V]**.

### 1.3 D3D device creation (0x4424c0)

* Form data: 640×480, backbuffer count 1, `D3DFMT_D16` (0x50) auto depth/stencil, `Flags=1` (`LOCKABLE_BACKBUFFER`), 32-bit X8R8G8B8 windowed by default. Reduced-colour conditions (config+0x1E == 1 **or** `Supervisor_IsReducedColour()` (`0x443a90`, flag bit 2) == 1, i.e. A8R8G8B8 previously unsupported) switch the windowed back buffer to `D3DFMT_R5G6B5` (0x17) **[V]**
* Swap behaviour **[V]**:
  * sync (default): `FullScreen_RefreshRateInHz = 60`, `PresentationInterval = D3DPRESENT_INTERVAL_ONE`, `SwapEffect = FLIP` when frame-skip (config+0x23) == 0, else `COPY_VSYNC`;
  * async (`g_AsyncPresent` 0x17ce8d4 == 1, set when the exe was launched from a shortcut whose start directory differs from the module directory (=`0x17ce71c`) or when the driver advertises the quirk bit 0x4000 in supervisor+0x150): `SwapEffect = COPY`, `PresentationInterval = IMMEDIATE (0x80000000)`. `0x17ce8d8` (default 1) is cleared when a device-creation retry happens with the async path **[V]**;
  * fullscreen: back-buffer format taken from `GetAdapterDisplayMode`, `SwapEffect = COPY`.
* `IDirect3D8::CreateDevice` slot used: +0x3C (index 15) **[V]**. Failure handling retries with adjusted swap parameters and logs (fatal for `IMMEDIATE` interval failures); the soft-T&L / reference fallback chain is HAL+0x40 → HAL+0x20 → REF (2) **[V]**.
* On success: `g_PresentParams` (0x17ce838) = copy of the 52-byte `D3DPRESENT_PARAMETERS` **[V]**; `g_DeviceFlags` (0x17ce8fc) bit0/bit1 set, bit0 = "hardware T&L path active" (log "T&L HAL で動作しま～す") **[V]**; initial world/view/projection matrices built (view `0x17ce7a0`, proj `0x17ce7e0`) with eye (320, −240, −z₀), target (320, −240, 0), up (0, 1, 0), fov 30° (0x3f060a92), aspect 4/3, zn = 100.0, zf = 10000.0, then `SetTransform(2/3)` and `SetViewport(0x17ce820)` **[V]**; `GetDeviceCaps` → `g_DeviceCaps` (0x17ce908) **[V]**; `CheckDeviceFormat(0, HAL, adapterFmt, 0, D3DRTYPE_TEXTURE, D3DFMT_A8R8G8B8)` → on failure log "D3DFMT_A8R8G8B8 をサポートしていません、減色モードで動作します" and set supervisor flag bit 2 **[V]**; caps dump: `MaxTextureWidth` (+0x58) < 512 warning, `TextureOpCaps & D3DTEXOPCAPS_ADD (0x40)` probe → on failure log + set supervisor flag bit 0 ("config already present" makes this probe normally dead, see NOTES open Q4) **[V]**; `RenderState_SetupDefault` (0x442f60, §6.5) ; clear/fade colour initialised by `0x45b0e0(0xFF000000)`; `g_QuitFlag = 0`, `0x17ce8dc = 0` **[V]**.

### 1.4 Restart / exit loop

The whole boot block sits in `do { … } while(true)` **[V]**. Restart begins by `Sleep(1000)`, stops sound (`0x45c4b0`), frees the AnmManager (tracked alloc/free `0x429df0` + `operator delete[]` via `0x441e10`), best-effort `Reset()` then `Release()` on device and D3D8 objects, hides/destroys the window, `ShowCursor(1)`. For a restart (`local_28 == 2`) it resets the log ring (`0x443990`), logs "再起動を要するオプションが変更されたので再起動します", pumps 60 messages, then jumps back into the boot block. For exit it calls `Config_Save(0x3c)` (`0x43e8f0`, writes the 60-byte config), frees the `0x17ce8e4` object, deletes the critical sections (`0x448a2f`), restores the three saved `SystemParametersInfo` values, re-enables IME and returns 0 **[V]**.

### 1.5 Config file (`th08.cfg`, 60 bytes)

* Loaded into `g_Config` (`0x17ce870`), 60 bytes, version magic `0x00080001` at +0x14 **[V]**. Validation requires `+0x14 == 0x00080001`, `+0x1C < 7`, `+0x1D < 4`, `+0x1E < 2`, `+0x1F < 3`, `+0x20 < 2`, `+0x21 < 6`, `+0x22 < 2`, `+0x23 < 3`, `+0x24 < 3`, `+0x25 < 2`, `+0x26 < 2`, and a 0x3C sanity value; on failure → "コンフィグデータが異常でしたので再初期化しました" + defaults; on missing data → "コンフィグデータが見つからないので初期化しました" + defaults **[V]**.
* Defaults written: `+0x1C=2`, `+0x1D=3`, `+0x1E=0`, `+0x18/+0x1A=600`, five key dwords `+0x00..+0x10` from `0x4c78c8`, `+0x24=2`, `+0x27=100`, `+0x28=0x50` **[V]**.
* The on-disk file matches this layout byte-for-byte (magic at 0x14 = `01 00 08 00`, 600/600 at 0x18/0x1A, `+0x23 = 0` etc.) **[V]**.
* `+0x1F` is the BGM mode: 1 = wave (thbgm.dat present and version-valid; header magic "ZWAV" `0x5641575A`, ver 1, entry size 0x800), 2 = MIDI fallback ("wave データが無いので、midi にします") **[V]**.
* Consumed fields used by the engine: `+0x22` fullscreen/windowed, `+0x23` frame-skip / render interval (0..2), `+0x1E` reduced colour, keys, BGM mode, `+0x20`-ish flags; the writer is the in-game option screen (`0x46837b` region) **[V]**.
* Config messages route through `Logger_Print` (`0x43ea00`) / fatal logger (`0x43eb60`) **[V]**.

### 1.6 Version / archive checks

`0x4461a0` builds `"th08_%.4x%c.ver"` with (0x100, 'd') → `th08_100d.ver`, loads it through `0x43e660(0)` (size kept at `0x17ceab4`), and verifies `th08.dat` existence (`0x4748c0`) **[V]**. `0x43e660(index)` is a small file/table dispatcher (indices 0 = version file, 1 = config) — its table was not dumped (NOTES open Q5) **[I]**.

---

## 2. Main loop

### 2.1 Message/device loop (inside WinMain)

```text
while (g_QuitFlag == 0) {
    if (PeekMessage(PM_REMOVE))  TranslateMessage + DispatchMessage;
    else {
        hr = IDirect3DDevice8::TestCooperativeLevel();           // vtbl+0x0C
        if (hr == D3DERR_DEVICENOTRESET) {                       // 0x88760869
            AnmManager_ReleaseResources(); Reset(&g_PresentParams);
            RenderState_SetupDefault(); g_DeviceLostCount = 3; set device-lost flag (0x17ce8fc |= 0x10);
        } else if (hr == 0) {
            rc = FrameStep();                                     // §2.2
            if (rc != 0) break;                                   // 1 = quit, 2 = restart
            clear device-lost flag;
        }
    }
}
after loop: sound shutdown (0x4582a0 if active), chain cleanup (0x43ce30), drain BGM until Bgm_Update returns 0;
then the restart/exit logic of §1.4.
```
**[V]** — the two error paths are as in the original decompilation; `0x43ce30` is a chain-coordinator teardown **[I]**.

### 2.2 Frame step `0x441e70` (ecx = main-state base `0x17ce700`)

```text
now = Time_GetSeconds(); state.now = now;
if (now < state.prev) state.deadline = now;         // clock went backwards → resync
state.prev = now;
if (now <= state.deadline) { Sleep(0); return 0; }  // not yet time for a logic tick
while (state.deadline < now) state.deadline += 1/60.0;   // fixed 60 Hz deadline, lag is dropped
AnmManager_Flush();                                  // 0x462e40
SetViewport(0, 0, 640, 480);                         // g_Viewport 0x17ce820, vtbl+0xA0
rc = Chain_RunCalc();                                // 0x43ca50 (GameSystem "update")
Bgm_Update();                                        // 0x45d790
if (rc ==  0) { cleanup; return 1; }                 // quit
if (rc == -1) { cleanup; return 2; }                 // restart
state.frameCounter++;
if (state.frameCounter >= (s8)config[0x23]) {        // render gate (§3)
    BeginScene(); AnmManager_ResetBatch(); fogFlag=0xFF;
    RenderHelper_SceneReset();                       // 0x448229
    Chain_RunDraw();                                 // 0x43cb60
    AnmManager_Flush(); SetTexture(0, 0); EndScene();
    state.frameCounter = 0;
}
state.now = Time_GetSeconds();
Present_Wrapper();                                   // 0x442060, see §2.4
return 0;
```
**[V]** for the sequence, **[I]** for the "lag dropped" one-line summary (the loop advances the deadline in `1/60` steps until it is ≥ now, so the tick count per frame is what it is; accumulated lag is *not* preserved).

### 2.3 Chain system ("GameSystem")

`g_Chain` = `0x164f548`. Two intrusive priority lists live inside it: **calc list** at `+0x00`, **draw list** at `+0x20`. **[V]**

Chain element (0x20 bytes, tracked alloc with tag `"funcChainInf"` from allocator context `0x164f598`): **[V]**

| off | type | meaning |
|-----|------|---------|
| +0x00 | s16 | priority (ascending; newly added elements go *after* equal priorities) |
| +0x02 | u16 | flags; bit0 = element is owned by the chain (freed on remove) |
| +0x04 | fn | per-frame callback (calc or draw, depending on list); `int __thiscall(ctx)` |
| +0x08 | fn | one-shot init callback, invoked by `Chain_Add` with ctx as `this`, then zeroed; its return value is the return value of `Chain_Add` |
| +0x0C | fn | remove callback |
| +0x10 | elem\* | prev |
| +0x14 | elem\* | next |
| +0x1C | void\* | context pointer passed to all callbacks |

API **[V]**: `ChainElem_Alloc(cb)` `0x43ce60`; `Chain_AddCalc(chain, elem, prio)` `0x43c880`; `Chain_AddDraw` `0x43c960` (same, head `+0x20`); `Chain_RunCalc` `0x43ca50`; `Chain_RunDraw` `0x43cb60`; `ChainElem_Remove` `0x43cf50` / `0x43cf10`; `ChainElem_SetCallback(cb)` `0x406520`. Adds/runs take the critical section at supervisor+0x298 index 0 (`Lock 0x43ef70` / `Unlock 0x43efb0`); the callback itself is invoked *unlocked*.

Callback return protocol (jump table at `0x43cb38`, 7 entries) **[V]**:
`0` remove this element and continue with the next; `1` continue with next (count++); `2` call this same element again immediately (loop until another value); `3` stop and return 1; `4` stop and return 0; `5` stop and return −1; `6` restart the scan from the list head. (`FrameStep` maps run-calc 0 → quit, −1 → restart; see §2.2.)

### 2.4 Present wrapper and snapshot (`0x442060`) **[V]**

```text
hr = Present(0, 0, 0, 0);                    // vtbl+0x3C, 4 args
if (hr < 0) { AnmManager_ReleaseResources(); Reset(&g_PresentParams);
              RenderState_SetupDefault(); g_DeviceLostCount = 2; }
AnmManager_TailHook();                       // 0x443b60, two intrusive lists at AnmManager+0x2a252c/+0x2a2550
if (snapshot key bit 0x800 at 0x164d528 rose):  mkdir("snapshot");
      first free name "snapshot/th%.3d.bmp" over 0..999 → write (0x4748f)
if (g_DeviceLostCount != 0 && 0x164d0ba == 0) g_DeviceLostCount--;
```
`g_DeviceLostCount` (0x17ce8cc) is a 2–3 frame grace counter consulted by loaders to blank out resources; `0x164d0ba` gates the decrement **[V]** (purpose of 0x164d0ba **[I]**).

---

## 3. Frame timing

* **Clock** `Time_GetSeconds` `0x442190` **[V]**: if `g_QPF == 0` → `timeBeginPeriod(1); t = timeGetTime(); timeEndPeriod(1)` and return `t`; else `QueryPerformanceCounter` and return `(double)low32 / (double)QPF`.
  * Note (**[I]**): the fallback branch returns **milliseconds**, not seconds (no division), so on a machine where `QueryPerformanceFrequency` returns 0 the game would tick ~1000× fast; this is best treated as a latent fallback quirk.
* **Logic rate**: fixed 60 Hz — the deadline advances in steps of the constant double `1/60` at `0x4b5bd8` **[V]**. At most one logic tick runs per iteration; if the process is late, the deadline is fast-forwarded (lag dropped) rather than a catch-up loop **[V]**.
* **Frame-skip / render interval**: byte `state+0x10` (address `0x17ce710`, seeded `0xFC`) is **pre-incremented**, then compared as *signed* against `(unsigned char)config+0x23` (`0x17ce893`). Render happens when `counter >= interval`; counter resets to 0. So interval 0/1 renders every tick (after the 4-tick 0xFC seed "debounce"), 2 renders every other tick **[V]**.
* **Sleep**: when the frame is not yet due the loop only calls `Sleep(0)` (yield) — no busy wait **[V]**.
* **Presentation rate** is therefore tied to vsync only in windowed/fullscreen sync modes (§1.3); the async mode presents immediately **[V]**.

---

## 4. Resolutions and coordinate systems

* **Back buffer / window**: always 640×480 (also fullscreen); windowed style `0xCF0000`, fullscreen style `0x100A0000` with frame-metric-adjusted client size **[V]** (see §1.2).
* **Screen-space viewport**: every logic tick sets the viewport to (0, 0, 640, 480) before the draw chain **[V]**.
* **Playfield viewport**: the stage draw task (`0x409200`) sets the viewport to X=32, Y=16, W=384, H=448 (`0x180`,`0x1C0`) and clears the target to opaque black; the companion task (`0x409640`) restores the full-screen viewport afterwards **[V]**. The playfield is therefore 384×448 pixels offset by (32, 16) in a 640×480 frame.
* **Projection**: perspective, full fov = 30° (`π/6`); device-init values zn = 100, zf = 10000, aspect 4/3; the stage camera (`0x40b6d0`) substitutes fov from stage field `+0x63DC` (default 30°), aspect = viewport width/height (384/448 during stage draw), zn = 30.0, zf = 1800.0 **[V]**. Both matrices are global: view `0x17ce7a0`, proj `0x17ce7e0`; `SetTransform(2)` and `(3)` (`D3DTS_VIEW`/`PROJECTION`) are re-issued by the stage camera **[V]**.
* **Camera model (stage)**: `0x40b6d0` builds basis vectors from stage fields `+0x6394`, `+0x63A0`, `+0x63AC`, `+0x63D0` (helper `0x409080`) and calls the look-at builder `0x478395`; the device-init call uses (eye (320, −240, −z₀), target (320, −240, 0), up (0, 1, 0)) with `z₀ = 240 / tan(fov/2)` **[V]** for the initial view, roles of the four stage vectors **[I]**.
* **2D sprite space**: sprites are emitted as pre-transformed `XYZRHW | DIFFUSE | TEX1` vertices (FVF `0x144`, stride 28 bytes) — i.e. a plain 2D top-left-origin pixel space over the 640×480 frame **[V]**. RHW = 1.0 for screen-space vertices **[I]**.
* **Sprite rotation** is **radians**: the rotation field of a sprite object is fed directly into `FSINCOS` (`0x46328d`) **[V]**. Corner geometry scales by the object's size/scale fields and divides by the float constant `0x4b42ec` = **2.0f** (half-extent), not an angle factor (correction of an earlier note) **[V]**.
* **Global screen-shake / offset**: `AnmManager+0x1C` (x) and `+0x20` (y) are added to every emitted sprite position in both sprite paths **[V]**; they are reset to 0 each frame by the boot task `0x445453` and driven by the shake helpers around `0x45be8a`/`0x45bed6` (`0`/`+v`/`−v` by flag) **[V]**.
* **Stage 3D coordinates**: playfield-bound code uses 384.0 (`0x43c00000`) and 448.0 (`0x43e00000`) constants near `0x40e5xx`/`0x411xxx` **[V]**; scripting/collision coordinate convention (y sign, world origin) is outside this sub-agent's scope — the only hard evidence here is the device-init view above **[I]**.

---

## 5. Subsystem / manager inventory

### 5.1 Registered chain elements (calc list / draw list)

All entries below are **[V]** (element address, priority, callback addresses read from the registration code). "Assets" column lists files the manager loads and the AnmManager slot it occupies (slots: 4=stgNbg, 5=player, 6=etama, 7=enemy, 8=stgNenm, 9=effect, 17=music00).

| Manager | Calc elem / prio / cb (init / remove) | Draw elems / prio / cb | Context object | Assets / notes |
|---|---|---|---|---|
| Boot scene | init-reset task cb `0x445453` prio 0 (0x445ea7 boot start; one-shot title logo `0x445fa3` loads `title/th08logo.jpg`; remove `0x446d06`) | `0x445bc0` prio 0x10, `0x445bd4` prio 0, `0x445d3e` prio 2 | uses `g_Supervisor` (0x17ce758) | creates the title/menu front end |
| Ascii/text | elem `0x4e3fd0` prio 1 cb `0x402830` (init `0x4028c0`) | `0x4cce00` prio 0x14 cb `0x4023d0`; `0x4e3ff0` prio 0xE cb `0x402430` | — | `ascii.anm` |
| Stage / stage script VM | elem `0x4ea650` prio 8 cb `0x407400` (init `0x409850`, remove `0x409c20`) | `0x4e4010` prio 6 cb `0x409200` (playfield 3D, §6.6); `0x4ea630` prio 7 cb `0x409640` (restore 2D) | `0x4e4030` (~0xB000+ bytes) | `stgNbg.anm` (slot 4), `stageN.std`/`stageNs.std`; camera in `0x40b6d0` |
| Effect manager | elem `0x577ebc` prio 0xD cb `0x427bf0` (init `0x4284b0`, remove `0x428590`) | `0x577edc` prio 0xC cb `0x427f00` | `0x4ece60` (653 slots, stride 0x360) | `eff0N.anm` (slot 9); slot fields +0x1F8 flags, +0x208 pos, +0x288 rot, +0x34C cb, +0x350 alive, +0x358 ANM |
| Ending | prio 5 (`0x4296a0` group; init `0x429980`/`0x429d40`) | prio 4 | — | ending sequence |
| Enemy manager | elem `0xf54e30` prio 0xB cb `0x42c660` (init `0x42ebf0`, remove `0x42ee80`) | `0x577f00` prio 8 cb `0x42e120`; `0xf54e50` prio 0xB cb `0x42eb90` | `0x577f20` | `enemy.anm` slot 7, `stgNenm.anm` slot 8, `ecldataN.ecl` |
| Bullet manager | elem `0x160f408` prio 0xE cb `0x431240` (init `0x433070`, removers `0x433820`/`0x4338c0`) | `0x160f4e70` prio 0xD cb `0x432b50` | `0xf54e90` | `etama.anm` slot 6 + 21 type × 5 sprite tables (stride 0x2A4); 1536 bullet slots |
| FRScreen infrastructure | elem `0x160f4e4` prio 0xF cb `0x4338ca` (init `0x437a2f`/`0x437a40`) | `0x160f4c4` prio 0x11 cb `0x433927` | — | 300-object counter mod |
| Game (pause / replay / stage lifecycle) | elem `0x164d344` prio 2 cb `0x439bc7` (Pause/UnPause; init `0x43aaf4`, remove `0x43be2c`) | `0x164d364` prio 5 cb `0x43aa03` | `0x160f508` buffer ptr; `0x164d2c8` | stage-start init `0x43abd7`: `timeGetTime`, stage bit masks, replay buffers `0x160f508/50C/510`, references `etama.anm` |
| Music room | prio 4 cb `0x44970f` | prio 3 | `0x5240`-byte "MusicCmtInf" | `music00.anm`, `sprt/musiccmt.txt`, `result/music.jpg` |
| Player | elem `+0xE2B10` of object `0x17d5ef8`, prio 9 cb `0x44c390` (remove `0x44dc60`; loader `0x44d650`) | `+0xE2B14` prio 9 cb `0x44d530`; `+0xE2B18` prio 0xA cb `0x44d630` | object ≈0xE2B30 bytes | `player00.anm` slot 5; radix/score helpers near `0x4a3e70` |
| Replay infra | creatures with calc prios 0x11/0x12/7/6 (`0x451f90` family object `0x18b8a28`, 0xDC bytes) | `0x18b8a0c` prio 9, `0x18b8a10` prio 10 | — | replay recording/playback support |
| Screen factory | dynamic prio 3 | — | "ScreenInf", 0x34 bytes, factory `0x45b8b0` | transient menu/notification screens |
| Sub-system A | elem `+0x46468` (object built at `0x4583d3`) prio 0x10 | `+0x4646C` prio 0x12 | object | purpose not yet named **[I]** |
| Sub-system B | elem `+0x14438` (object built at `0x47146d`, 0x14490 bytes) prio 4 | `+0x1443C` prio 3 | object | purpose not yet named **[I]** |

### 5.2 Non-chain subsystems

| Subsystem | Object / global | Evidence |
|---|---|---|
| Supervisor (app state, D3D, locks, caps flags) | `0x17ce758` | **[V]** |
| AnmManager (sprite/animation master, 0x2a2570 bytes) | `0x18bdc90` | **[V]** |
| Sound / BGM (DirectSound8, 0x5224 bytes) | `0x18b8a68` | **[V]** |
| GameSystem / chain | `0x164f548` | **[V]** |
| Logger (ring buffer 8 KB at ctx+0x2000) | `0x164d540` | **[V]** |
| Main loop state (hwnd + timing + frame counter) | `0x17ce700` | **[V]** |
| Tracked heap (tagged allocations, e.g. "SpriteInf") | `0x164f598` | **[V]** |

### 5.3 Input

* Keyboard: state init `0x43e160`; the per-frame snapshot key test reads a 16-bit field at `0x164d528` against the previous field at `0x164d530` (bit `0x800` = snapshot) **[V]**.
* Joystick: `0x43d0c0`, state at `0x164d388` (`joyGetPosEx`/`joyGetDevCapsA`) **[V]**.
* DirectInput 8: created lazily through the supervisor's deferred-call mechanism (`0x4486ba(0x4464c8, …)` invoked from the boot task); `DirectInput8Create(hInstance, 0x800, IID_IDirectInput8A @0x4bd6c0, &obj+0x0C, 0)`, keyboard `CreateDevice(GUID_SysKeyboard @0x4bd620, &obj+0x10, 0)`, `SetCooperativeLevel(hwnd, 0xFFFFFFFA)` **[V]**. A small 0x14-byte helper object at `0x17ce86c` (ctor `0x443df0`) sits beside it **[V]** (role **[I]**).

---

## 6. Rendering pipeline

### 6.1 Per-frame order (single-threaded, all in FrameStep) **[V]**

1. `AnmManager_Flush()` — draws whatever was queued before the update (normally empty).
2. Logic update of the calc chain (`Chain_RunCalc`).
3. If render gate open: `BeginScene` → `AnmManager_ResetBatch` → `fogFlag=0xFF` → `RenderHelper_SceneReset (0x448229)` → draw chain (`Chain_RunDraw`) → `AnmManager_Flush` → `SetTexture(0, NULL)` → `EndScene`.
4. `Present_Wrapper` → `Present` + device-lost handling + snapshot + AnmManager tail hook.

Layers are ordered purely by draw-list priority (ascending; table in §5.1). The stage draw task itself performs the playfield viewport switch and its own `Clear` before its 3D content, and the following task restores the full-screen viewport (§4).

### 6.2 D3D8 interfaces used (observed vtable slots) **[V]**

`IDirect3D8` (object `g_pD3D8`): `GetAdapterDisplayMode` +0x20 (idx 8), `CheckDeviceFormat` +0x28 (idx 10), `CreateDevice` +0x3C (idx 15).
`IDirect3DDevice8` (object `g_pD3DDevice`): `TestCooperativeLevel` +0x0C, `GetDeviceCaps` +0x1C, `Reset` +0x38, `Present` +0x3C, `BeginScene` +0x88, `EndScene` +0x8C, `Clear` +0x90, `SetTransform` +0x94, `SetViewport` +0xA0, `SetRenderState` +0xC8, `SetTexture` +0xF4, `SetTextureStageState` +0xFC, `DrawPrimitive` +0x118, `DrawPrimitiveUP` +0x120, `SetVertexShader` +0x130, `SetStreamSource` +0x14C. All were confirmed by argument count (Reset takes 1 pointer, Present takes 4 pointers, etc.) — no slots outside this set are used.

### 6.3 AnmManager geometry batching **[V]**

* Inline batch arena: count `+0x2520` (quads), write cursor `+0x2A2524`, batch start `+0x2A2528`; arena starts at `AnmManager+0x2524` and runs to the object end (~2.6 MB).
* `AnmManager_ResetBatch` `0x462e00`: count = 0; cursor = `this+0x2524`; batch start = cursor.
* Quad emit `0x462f10`: copies 4 corner vertices (28 bytes each) as 6 vertices (`v0,v1,v2,v1,v2,v3`) into the arena, advances cursor by 0xA8, count += 1.
* `AnmManager_Flush` `0x462e40`: if count == 0 return; `SetTextureStageState(0, D3DTSS_ALPHAARG2(6), D3DTA_DIFFUSE(0))`; `(0, D3DTSS_COLORARG2(3), 0)`; `SetVertexShader(FVF 0x144 = XYZRHW|DIFFUSE|TEX1)`; `DrawPrimitiveUP(TRIANGLELIST, 2·count, batchStart, stride 28)`; batch start = cursor; count = 0; frame counter `+0x18`++.
* Screen-space sprite path `0x463210`: rotation (radians) via `FSINCOS`, half-extents from size×scale/2, adds `AnmManager+0x1C/+0x20` (screen shake), emits through the quad helper.
* Matrix sprite path `0x464470` (sprite objects that carry matrices): sets `WORLD` (`0x100`) from `obj+0x170`; sets `TEXTURE0` (`0x10`) from `obj+0x1B0` plus texcoord offsets (`tex+0x20/24`, `obj+0x30/34`) — skipped when the same texture object and a zero u-offset are reused (`AnmManager+0x24C8` caches the last matrix-path texture). Binds the texture when `AnmManager+0x24BC` (cached `IDirect3DTexture8*`) differs from `texObj+0x4`. Path state byte `AnmManager+0x24C2` guards the one-time-per-batch setup: normal path sets `SetVertexShader(FVF 0x102 = XYZ|TEX1)`, `SetStreamSource(0, AnmManager+0x24CC, stride 20)`, and draws `DrawPrimitive(TRIANGLESTRIP, 0, 2)` from the inline vertex buffer; when supervisor flag bit 1 is set (vertex-buffer suppression, currently unreachable — see NOTES) it switches to `SetVertexShader(0x142 = XYZ|DIFFUSE|TEX1)` and `DrawPrimitiveUP(TRIANGLESTRIP, 2, &0x18bdd68, stride 24)`. In both cases stage states `(0, ALPHAARG2=6, 3)` and `(0, COLORARG2=3, 3)` are set (`3` = `D3DTA_TFACTOR`).

### 6.4 HUD/text and 3D notes

* Text/ASCII rendering goes through the same quad batching via `ascii.anm` (Ascii manager, §5.1) **[V]**.
* The stage path uses the transformed pipeline (world/view/projection) for backgrounds and 3D props; sprites drawn during the stage are still screen-space quads unless they use the matrix path **[V]**.
* Backbuffer clearing: the main frame never clears the full target; the stage draw task clears the *viewport* to `0xFF000000` before 3D content, and the frame behind it is normally fully covered by the 2D layers **[V]** (reliance on full coverage **[I]**).

### 6.5 Render states

Default setup `RenderState_SetupDefault` `0x442f60` (applied at device creation and after every Reset) **[V]**:

| State | Value | Meaning |
|---|---|---|
| `D3DRS_ALPHABLENDENABLE` (27) | 1 | alpha blending on |
| `D3DRS_SRCBLEND` (19) | 5 | `D3DBLEND_SRCALPHA` |
| `D3DRS_DESTBLEND` (20) | 6 | `D3DBLEND_INVSRCALPHA` |
| `D3DRS_ALPHATESTENABLE` (15) | 1 | alpha test on |
| `D3DRS_CULLMODE` (22) | 1 | `D3DCULL_NONE` |
| `D3DRS_ZFUNC` (23) | 8 | `D3DCMP_ALWAYS` |

Fog and per-scene state helpers: `0x448270` = flush + render-state command sequence (fog enable 0x1C / fog colour 0x22 / fog start 0x24 / end 0x25) via `SetRenderState`; `0x448229` = flush + restore fog/render state after clearing the supervisor fog flag (`+0x350`) **[V]**. Stage load sets fog colour `0xFF000000`, fog start 200.0, fog end 500.0 by default **[V]**. The per-frame `fogFlag (=_DAT_017ceaa8) = 0xFF` write in FrameStep is a "dirty" marker consumed by these helpers **[I]**.

### 6.6 Stage 3D

* Stage load `0x409850`: `stgNbg.anm` into slot 4, `stageN.std`/`stageNs.std`, default fog, initialises camera vectors (`+0x6394`, `+0x63A0`, `+0x63AC`, `+0x63D0`) and the fov field `+0x63DC` (30° default) **[V]**.
* Camera update `0x40b6d0`: rebuilds view `0x17ce7a0` (look-at via `0x478395`/`0x409080`) and proj `0x17ce7e0` (fov, aspect = viewport w/h, zn 30, zf 1800, via `0x4784dd`), then `SetTransform(2)`/`SetTransform(3)` **[V]**.
* Playfield draw `0x409200`: `SetViewport(32,16,384,448)`, `Clear(0, NULL, D3DCLEAR_TARGET, 0xFF000000, 1.0, 0)`, background and 3D objects, drawn under the 2D layers by priority **[V]**.
* Sprite billboards in 3D use the matrix path (§6.3) — the same functions serve 2D and 3D sprites **[V]**.

### 6.7 Capability switches affecting the pipeline

Supervisor flags (`+0x150`, see §7): bit 0 "colour-add emulation" (set by Config_Load; also set by device init when `D3DTEXOPCAPS_ADD` is missing) **[V]**; bit 1 "suppress vertex buffers" — read by the sprite matrix path (`0x443a70`) but **no writer exists in the binary** (dead switch) **[V]**; bit 2 "reduced colour / no A8R8G8B8" **[V]**; bit 9 "force reference rasterizer" — getter `0x443ab0` only, no writer (dead) **[V]**. Corresponding operator messages are logged at config load / device creation.

---

## 7. Globals table (principal)

All `[V]` unless marked. Sizes are the ones directly evidenced; "…" = larger/uncertain extent.

| Address | Name | Type / size | Meaning |
|---|---|---|---|
| `0x17ce6fc` | `g_hInstanceMutex` | HANDLE | instance mutex ("Touhou 08 App") |
| `0x17ce700` | `g_MainState` | struct, ≥0x48 | +0x00 HWND (main window); +0x10 u8 frame counter (seed 0xFC); +0x2C double now; +0x34 double prev; +0x3C double deadline |
| `0x17ce704` | `g_QuitFlag` | u32 | nonzero ends the main loop |
| `0x17ce708` / `0x17ce70c` | `g_WindowModeA/B` | u32 | set to 1/0 at window creation; windowed/fullscreen bookkeeping **[I]** |
| `0x17ce710` | (in `g_MainState`) | u8 | render-gate counter, seed 0xFC |
| `0x17ce714` | `g_QPF` | u64 | QueryPerformanceFrequency |
| `0x17ce71c` | `g_LaunchDirMismatch` | u32 | set when the process start directory ≠ module directory (shortcut with "start in") |
| `0x17ce720/4/8` | `g_SavedSPI[3]` | u32×3 | saved screen-saver/low-power settings to restore |
| `0x17ce758` | `g_Supervisor` | struct, ≥0x350 | +0x00 HINSTANCE; +0x04 `IDirect3D8*`; +0x08 `IDirect3DDevice8*`; +0x44 HWND copy; +0x13A u8 fullscreen flag (readers only, no static writer — NOTES Q1); +0x150 u32 capability flags; +0x17C u32; +0x298 4×CRITICAL_SECTION (stride 0x18); +0x350 u8 fog/state dirty flag |
| `0x17ce75c` | `g_pD3D8` | `IDirect3D8*` | |
| `0x17ce760` | `g_pD3DDevice` | `IDirect3DDevice8*` | |
| `0x17ce79c` | (supervisor+0x44) | HWND | window handle copy |
| `0x17ce7a0` | `g_matView` | D3DMATRIX | current view |
| `0x17ce7e0` | `g_matProj` | D3DMATRIX | current projection |
| `0x17ce820` | `g_Viewport` | D3DVIEWPORT8 | X/Y/W/H floats updated by main loop and stage |
| `0x17ce838` | `g_PresentParams` | D3DPRESENT_PARAMETERS (52 B) | used for `Reset` |
| `0x17ce86c` | `g_InputHelper` | 0x14 B object | sits next to DirectInput object (role **[I]**) |
| `0x17ce870` | `g_Config` | 60 B | see §1.5; `+0x23` = render interval, `+0x22` fullscreen, `+0x1E` reduced colour, `+0x1F` BGM mode, `+0x24..+0x28` misc options |
| `0x17ce8a8` | `g_SupervisorFlags` | u32 | = supervisor+0x150 (see above) |
| `0x17ce8cc` | `g_DeviceLostCount` | u32 | 2–3 frame grace counter, decremented in Present wrapper |
| `0x17ce8d4` | `g_AsyncPresent` | u32 | 1 = COPY/IMMEDIATE present path |
| `0x17ce8d8` | `g_PresentRetryFlag` | u32 | default 1, cleared on device-creation retry **[I]** |
| `0x17ce8dc` | — | u32 | cleared at device creation end **[I]** |
| `0x17ce8e4` | `g_ExtraObject` | ptr | freed during restart/exit |
| `0x17ce8fc` | `g_DeviceFlags` | u32 | bit0 HW-T&L path, bit1 device created, bit2 A8R8G8B8 texture OK, bit4 device-lost, others TBD |
| `0x17ce908` | `g_DeviceCaps` | D3DCAPS8 | copied from device; `MaxTextureWidth` at +0x58, `TextureOpCaps` at +0x90 |
| `0x164d528` / `0x164d530` | `g_InputState` / `g_InputStatePrev` | u16 each (8 B apart) | current/previous input field; snapshot on bit 0x800 rising edge |
| `0x164d540` | `g_Logger` | ctx | print context; 8 KB ring at +0x2000 |
| `0x164f548` | `g_Chain` | GameSystem | calc list head +0x00, draw list head +0x20 |
| `0x164f598` | `g_HeapCtx` | ctx | tracked allocator context for tagged allocations |
| `0x18b8a68` | `g_Bgm` | 0x5224 B | DirectSound8/BGM/streaming state |
| `0x18bdc90` | `g_AnmManager` | 0x2A2570 B | sprite/animation master (§6.3) |
| `0x4b5bd8` | `kFrameStep` | double | 1/60 |
| `0x4b5c30` | `kWindowTitle` | char[] | "東方永夜抄　～ Imperishable Night. ver 1.00d" |
| `0x4b5c60` | `kWindowClass` | char[] | "BASE" |
| `0x4b5b94` | `kLogBanner` | char[] | "東方動作記録 ----…" |
| `0x17ceab4` | `g_VerFileSize` | u32 | size of the loaded `th08_100d.ver` entry **[I]** |

Other well-known globals: callee-side constants and string addresses used above (`0x4b5f28` first-run message, `0x4b6cxx` config messages, `0x4bd620` GUID_SysKeyboard, `0x4bd6c0` IID_IDirectInput8A, `0x4c78c8` default key config) are documented at their use sites.

---

## 8. Pointers for the parent merge

* Proposed symbol list: `symbols_draft.csv` in this directory (function/data rows with signatures, confidence and notes).
* Coverage of this document: startup chain, frame loop/pacing, coordinate systems, complete chain-element registration inventory, rendering pipeline & states, principal globals.
* Not covered here (other sub-agents' scope): ANM file format details, ECL/stage script opcodes, bullet/effect semantics, BGM streaming internals, archive/format details.
* Open questions are listed in `NOTES.md` (§ "Open questions"); the four most load-bearing uncertainties are: fullscreen flag writer (Q1), AnmManager tail lists (Q3), supervisor bit-0 semantics (Q4), and the file-table loader behind `0x43e660` (Q5).
