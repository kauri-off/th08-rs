# NOTES — overview sub-agent (TH08 engine architecture)

Workdir: `/home/anon/Projects/th_08/specs/deepseek/sub/overview/`
Binary: `th08.exe` v1.00d — PE32, image base 0x400000, SHA256 `330fbdbf58a710829d65277b4f312cfbb38d5448b3df523e79350b879213d924`.
Source dir (read-only): `/home/anon/Downloads/Touhou Collection/TH08 - Imperishable Night/TH08 - Imperishable Night/`.

Deliverables in this dir: `00_overview.md` (main), `symbols_draft.csv` (for parent merge), this file.
Tooling: own Ghidra project copy at `./ghidra` (pre-analysed) + `./gh.sh <workdir> <Script.java> <args…>` + `./ghidra_scripts/`
(Decompile/Xrefs/Strings/ApplySymbols/CallEdges/DumpFuncs). Scratch assets live in `/tmp/opencode/`:
`full_disasm.txt` (complete objdump), `rva.py` (VA→bytes, Shift-JIS aware), `d3d8.h`/`d3d8types.h`
(Wine headers used as ABI reference for the runtime d3d8.dll vtable).

> Housekeeping: at 00:04 an external cleanup removed the intermediate dump files that used to live in this
> directory (`core_/init_/sub_/chain_/stage_/manager_/anm_/misc_decompiles.txt`, `all_strings.txt`, etc.).
> All facts below were re-verified against `/tmp/opencode/full_disasm.txt` and are reproducible with the
> tooling noted above; nothing in this directory needs to be reconstructed for the deliverable.

Tag semantics used everywhere: **VERIFIED** = read directly from disassembly/decompile or from an
external ABI reference and cross-checked at the call site; **INFERRED** = consistent with surrounding
evidence but no direct proof yet.

## Log

### Session 1 — 2026-10-16 (startup chain)
- Copied Ghidra project; dumped all strings (`all_strings.txt`) and function list (`funcs.txt`).
- PE entry 0x4a619e (CRT) → WinMain 0x4418c0 (VERIFIED).
- Mapped startup in order: SystemParametersInfoA save/disable pair → Supervisor_InitLocks 0x4489f9
  (4 × CRITICAL_SECTION at supervisor+0x298, stride 0x18) → Logger ctx init 0x43ea00(&DAT_0164d540, …)
  → instance mutex 0x443420 ("Touhou 08 App", GetLastError==0xB7 → fatal) → Config_Load 0x4477b1("th08.cfg")
  → QueryPerformanceFrequency → restartable `do{}` boot block: D3D8_Create 0x442200 → Window_Create 0x442240
  → D3DDevice_Create 0x4424c0 → Sound_Init 0x45c190 → Joystick_Init 0x43d0c0 → KeyboardState_Init 0x43e160
  → AnmManager alloc 0x465070 (`new(0x2a2570)` + tag "SprtCtrlInf") → window apply 0x443ad0 → Boot_Start 0x445ea7
  → message/frame loop while `DAT_017ce704==0`.
- Config file decoded end-to-end (60 bytes at 0x17ce870; magic 0x00080001 at +0x14; defaults; validation ranges;
  thbgm.dat 16-byte header magic "ZWAV"/ver 1/0x800).

### Session 2 — 2026-10-16/17 (frame loop, device, subsystems, render)
- Frame step 0x441e70, time source 0x442190, present wrapper 0x442060 decoded line by line (see 00_overview.md).
- D3D8/D3D8-device vtable offsets cross-checked against Wine `d3d8.h` and by argument arity at every call
  site (Reset vs Present disambiguated by arity 1 vs 4; full map in the overview).
- Device creation 0x4424c0: formats, swap effects, intervals, async-present flag, soft-TnL fallback chain,
  caps query, A8R8G8B8 CheckDeviceFormat probe, initial view/proj matrices, default render states 0x442f60.
- Chain/GameSystem fully mapped: element 0x20B "funcChainInf", add/run/remove 0x43c880/0x43c960/0x43ca50/0x43cb60,
  return-code protocol 0..6; complete registration inventory for all managers (table in overview).
- AnmManager geometry path decoded: batch arena at +0x2524 (inline, ~2.6 MB), emit 0x462f10 (4 corners → 6 verts of
  0x1c bytes), flush 0x462e40 (two texture-stage resets, FVF 0x144, DrawPrimitiveUP(TRIANGLELIST, 2·quads, stride 0x1c),
  frame counter++), sprite paths 0x463210 (fsincos on object rotation — radians) and 0x464470 (world/tex matrices +
  VB path with DrawPrimitive(TRIANGLESTRIP, 2) / UP fallback).
- Audio, input (DirectInput8), playfield viewport (32,16,384×448), camera (0x40b6d0) and screen-shake verified.

### Session 3 — 2026-10-17 (deliverable writing, spot re-verification)
- Re-verified before writing: d3d8 device vtable map (all 11 offsets used by the game), Reset=0x38/Present=0x3C by
  argument arity, FVF 0x144/stride 0x1c at flush, batch arena reset semantics (0x462e00), stage viewport writes,
  stage camera zn/zf (30/1800), initial device proj zn/zf (100/10000, fov π/6), window class "BASE" + title string,
  config-on-disk layout vs decodable fields, supervisor+0x150 flag writers (only `|=1` twice and `|=4` once exist —
  bits 1 and 9 have readers but no writers).
- Corrected two earlier mislabels: `FUN_00443ad0` is `Supervisor_IsFullscreen` (reader of +0x13A), not the setter;
  constant 0x4b42ec is 2.0f (half-extent divisor in sprite-corner math), not an angle scale — sprite rotation is
  radians fed straight to FSINCOS.
- Wrote `00_overview.md` + `symbols_draft.csv`.

## Core facts (quick reference; details in 00_overview.md)

- Main loop: `while (g_Quit==0) { if no message: TestCooperativeLevel → ok: FrameStep() / D3DERR_DEVICENOTRESET:
  release resources + Reset + render-state re-setup };`
- FrameStep 0x441e70: now=Time_GetSeconds(); if now<prev → deadline=now; prev=now; if now<=deadline → Sleep(0) and
  skip; else advance deadline by 1/60 steps (lag dropped), flush AnmManager, viewport(0,0,640,480), Chain_RunCalc
  (rc 0 → quit / −1 → restart), Bgm_Update, then if ++frameCounter >= (s8)config+0x23 → BeginScene, batch reset,
  fog flag 0xff, scene-state reset, Chain_RunDraw, flush, SetTexture(0,0), EndScene, counter=0, Present wrapper.
- Time: QPF present → QueryPerformanceCounter.low / QPF; absent → timeBeginPeriod(1)+timeGetTime() (ms only) — this
  path returns milliseconds (not seconds), likely a latent oddity/fallback bug (VERIFIED code, INFERRED impact).
- Device: 640×480, backbuffer count 1, D16 auto depth (0x50), LOCKABLE_BACKBUFFER; windowed X8R8G8B8 / R5G6B5 when
  reduced-color (supervisor flag bit 2) or config+0x1e; vsync ONE+FLIP, or COPY_VSYNC when frame-skip off, or
  IMMEDIATE+COPY when async (shortcut-dir mismatch or caps bit 0x4000); fullscreen = display format + COPY.
- Supervisors/globals: principal ones in the overview globals table (g_Supervisor 0x17ce758, g_AnmManager 0x18bdc90,
  g_Bgm 0x18b8a68, g_Chain 0x164f548, g_MainState 0x17ce700, g_Logger 0x164d540).

## Open questions

1. `supervisor+0x13A` (fullscreen flag): only readers found statically (0x443ad0, plus the window creator);
   no static writer — likely written through a pointer parameter or during mode switch in a function that
   copies the supervisor struct. Marked INFERRED in the docs.
2. `supervisor+0x150` bits 1 (suppress vertex-buffer path) and 9 (force reference rasterizer) have getters
   (0x443a70 / 0x443ab0) and log strings but no writers anywhere in .text — effectively dead switches.
3. Two linked lists at the AnmManager tail (roots +0x2a252c / +0x2a2550) processed each present by 0x443b60
   via 0x466f20 / 0x467180: mechanics VERIFIED, purpose (pending ANM/instance release) INFERRED.
4. `Supervisor+0x150` bit 0 is set unconditionally by Config_Load (0x447a5f) *and* by device init when
   D3DTEXOPCAPS_ADD is unsupported; because the config runs first, the device-init ADD probe is effectively
   skipped in a normal boot (flag already set). Exact intent (persistent "color-add emulation" vs "config ok")
   unresolved.
5. `0x4461a0` version check: builds "th08_%.4x%c.ver" (0x100,'d') → "th08_100d.ver" and loads entry 0 via the
   table loader 0x43e660; the loader's file table is not yet dumped (only indices 0/1 observed).
6. `FUN_00443420` in WinMain's first-pass condition (returns −1 ⇒ fatal path): identity only partly resolved
   (it is not the mutex check, which is 0x443420) — likely D3D-less "config presence" pre-check; left unsymbolised.
7. Frame-skip semantics: counter pre-incremented then compared `>=` — off-by-one relative to "render every Nth
   frame" wording; behaviour captured exactly in the overview pseudocode, config meaning (0/1/2) as stated.
