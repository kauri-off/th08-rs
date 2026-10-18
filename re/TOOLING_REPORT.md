# RE tooling report (RE_TOOLING_BRIEF deliverables 1–3)

Date: 2026-10-17. Ghidra 12.1.2 (/opt/ghidra), rea-agents 6.5.0.

## 1. Always-fresh dump (`tools/redump.sh`): done in phase 6

- Runs ApplySymbolsOpus, then DumpAll2. Outputs: `extracted/all_decomp.c`, `callgraph.tsv`, `globals_xref.tsv`. Takes about 50 s.
- Current dump: 2254 functions, **1306 still `FUN_`** (1322 after the first run). symbols.csv has 617 func, 25 global.
- `globals_xref.tsv` only sees absolute-address operands. It does not list struct-relative accesses (`g_Stage+0x625C` and similar).

## 2. rea pilot

### Setup and timings

| step | result |
|---|---|
| `open_binary` (provider `ghidra`) | instant, but lazy |
| first query (`address_name`) | **~41 s**: rea imports the exe into a new ephemeral project and runs full default auto-analysis. It does **not** use `re/ghidra/th08.gpr`. |
| `set_addresses_names` with all 641 symbols.csv entries | all 641 returned `true`, including non-entry labels such as `Music_Play` 0x447D56 (rea created functions or labels there). About 70 s wall, mostly spent emitting the 19 KB JSON payload. The response echoes the map twice (~40 KB of context). |
| `procedure_pseudo_code` by **name** (`Player_DamageAgainst`) | ~0.1 s |
| `batch_decompile` (3 functions) | <0.1 s |
| `xrefs` 0x425430 | <0.1 s, 93 call sites |
| `search_bytes` (whole image) | about 1 s, each match tagged with its containing function name |
| `get_call_graph` backward | instant; returns addresses only (no names); stops at indirect calls (func-chain) |
| compare: `gh.sh Xrefs.java` | 3.5 s per call; `fn.sh` 0.03 s |

### Names stick?
Yes, but only within the session. Renamed callees appear by name in later pseudocode (`Effect_Spawn`, `Vec2_Rotate`, `ItemMgr_Spawn`, `g_GameSpeed`), and procedures can be addressed by name. Names are lost on `close_binary` or when the MCP server restarts. The next session must re-import (~41 s) and re-apply the names (~70 s).

### Quality vs `gh.sh` / `all_decomp.c`
rea's decompile is **worse** than our project, because our project has had `FixThiscall.java` applied and rea's fresh import has not:
- `this` appears as `in_ECX`, and fastcall `EDX` appears as `in_EDX` (e.g. `PlayerBullet_HitSpark8`).
- Calls to thiscall helpers lose their arguments. `FUN_0040b460(&local_34)` becomes `FUN_0040b460()`, and `FUN_00451ce0(&box, pos, size)` loses its out-pointer. The locals it fills then look uninitialized, which makes the box-overlap test in `Player_DamageAgainst` hard to read.
- No signatures from symbols.csv are applied. `set_addresses_names` sets names only.
The control flow and constants were identical. Both versions show the same `Removing unreachable block 0x451C54` warning.

### Re-check of answers_impl9 Priority 1 (type-3 hit spark): CONFIRMED, now stronger
Using rea only (`procedure_pseudo_code`, `xrefs`, `search_bytes`, `get_call_graph`):
- The spawn condition matches the answer. Area +0x3C must be nonzero, `life(+0x24) % period(+0x38) == 0`, and the area must overlap. Then if +0x3D == 0, increment the u8 counter player+0xE2A94; when `(counter & 3) == 0`, spawn `Effect_Spawn(3, pos, 1, 0xFFFFFFFF)`. The `idx < 0xC0 ? 3 : 5` test is always true inside the loop.
- The only direct caller is `EnemyMgr_Calc` (`get_call_graph` backward).
- **New: exhaustive type audit.** The call pattern is `push type; mov ecx,0x4ECE60; call`. Byte pattern `6A 03 B9 60 CE 4E 00 E8` has exactly 1 match: 0x451C6E in `Player_DamageAgainst`. A cross-check of all 133 `mov ecx,EffectMgr; call` sites (rea pattern hits, decoded with `tools/pe.py`) found 12 sites with a runtime type. None can be 3:
  - `Enemy_DropItems` / `EnemyMgr_Calc`: e+0x3310 (signed byte) and e+0x3311+4. Only ECL op 138 writes these, and op 138 appears **0 times** in all 24 ECL files (`ecl_dis.txt`), so the types are 0 and 4.
  - `Ecl_Run` 0x41D01D / 0x41D174: op 139/140 ids in the data are only 17, 26, 40, 51, 60, 61 and 63.
  - `Ecl_Run` 0x41E5ED (`Effect_SpawnHigh`): aura 0x20+n (≥ 32).
  - `Spell_Start` 0x415602 (`Effect_SpawnFixed`): 39 or 52.
  - The others are draw or list helpers (`EffectMgr_Draw*`, `GetFixed` 9/10), not spawns.
  The original answer is VERIFIED including data-driven paths. No spec change is needed.

### Verdict
- **Use rea for:** fast `xrefs`, `search_bytes` with function attribution, call graphs, and ad-hoc decompiles of a few functions once a session is warm. All are sub-second, compared with 3.5 s per `gh.sh` call.
- **Do not use rea as the main reader.** The output lacks thiscall fixes and signatures, setup costs about 2 min per session, renames are lost, and responses are verbose (each result repeats ~1.5 KB of profile and limitations).
- `all_decomp.c` + `fn.sh` remains the best reader: 0.03 s, has `this`, has names. `gh.sh` remains the authority for anything that needs the fixed project.

### Runtime trace (not done; wine is not installed)
- Needs `wine` (32-bit, wow64) + d3d8 → `d3d8to9`/DXVK or wined3d. The game uses DirectX 8. It also needs the game folder copied to a scratch dir, because the game writes score.dat, th08.cfg and replays.
- Breakpoints: `gdb` exists (`/usr/bin/gdb`). It could attach to the wine process, or use `winedbg --gdb`, which is missing. A Frida session (rea `start_frida_session`, Frida is listed available) is easier: hook `Effect_Spawn`, `Rng_Next16`, `Player_DamageAgainst` and log arguments per frame.
- `apitrace` (missing) would capture the D3D8 call stream, which is useful for render-state and draw-order questions.
- Best first use: dump RNG state + call counter per frame during a demo replay. That gives a ground truth for replay-sync tests of the port.

## 3. Proposed edits to `docs/briefs/RE_WORKFLOW.md`

```diff
 ## Tools (`re/tools/`)
-- `redump.sh` — apply `re/symbols.csv` to Ghidra, re-export `re/extracted/all_decomp.c` (`===== name @ addr`), `callgraph.tsv`, `globals_xref.tsv` (~50 s). Run after every batch of new symbols.
+- `redump.sh` — apply `re/symbols.csv` to Ghidra, re-export `re/extracted/all_decomp.c` (`===== name @ addr`), `callgraph.tsv`, `globals_xref.tsv` (~50 s). Run after every batch of new symbols and at the start of a run if all_decomp.c is older than symbols.csv.
-- `fn.sh <addr|name>...` — print function(s) from the dump; `fnc.sh` — same plus `_DAT_` constants annotated as f32/i32.
+- `fn.sh <addr|name>...` — print function(s) from the dump (0.03 s; the default way to read code); `fnc.sh` — same plus `_DAT_` constants annotated as f32/i32.
+- Callers/callees: `grep` `extracted/callgraph.tsv` (columns addr, name, callers, callees) before running any Ghidra script. Indirect calls (func-chain, vtables, effect/bomb tables) are not listed.
...
-- REA MCP (`mcp__rea__*`) is also available; its renames are session-scoped, so persist names in symbols.csv.
+- REA MCP (`mcp__rea__*`, provider `ghidra`): a fresh ephemeral import, not `re/ghidra`. The first query takes ~40 s. There is no thiscall fix and no signatures (`this` shows as `in_ECX`, helper args are dropped), so do not quote its pseudocode for struct offsets without checking `fn.sh`.
+  Good for: `xrefs` (<0.1 s vs 3.5 s gh.sh), `search_bytes` (whole image, matches tagged with function name), `get_call_graph`.
+  Names: `set_addresses_names` with a JSON map built from symbols.csv works (641/641) but is lost on close/restart and costs ~40 KB of context. Apply only the names you need, or none.
+- "Only call site / only writer" claims: prove them with a byte-pattern search (e.g. `6A 03 B9 60 CE 4E 00 E8` = push 3; mov ecx,EffectMgr; call), then check every non-constant site against the data dumps (ecl_dis, anm_dis…). A constant-only search is not enough.
```

```diff
 ## Persistent memory (survives compaction)
 ...
+4. `re/TOOLING_REPORT.md` — tool timings/limits; read once per run, not after every compaction.
```

## 4. Runtime on wine (phase 8) — WORKS

Wine runs the original th08.exe headless. Game logic runs at 60 fps on wined3d with Mesa llvmpipe under Xvfb.

### Layout (`re/work/`, not in git)
| path | content |
|---|---|
| `env.sh` | `WINEPREFIX=re/work/prefix` (own win64 prefix), `WINE=~/PortProton/data/dist/WINE_LG_11-1/bin/wine` |
| `game/` | a COPY of th08.exe, th08.dat, thbgm.dat, score.dat, th08.cfg. The cfg is patched: +0x22 window = 1, +0x1F music = 0, +0x20 SE = 0. The thcrap/vpatch files are not copied. |
| `hook/hook.c`, `hook/d3d8.def` | the probe, a proxy `d3d8.dll` (build command below) |
| `run.sh <secs>` | starts Xvfb :77 if it is not running, copies the probe into game/, runs `wine th08.exe` with `WINEDLLOVERRIDES=d3d8=n,b` for <secs>, then `wineserver -k` |
| `cmds/*.txt` | probe command scripts; copy one to `game/probe_cmd.txt` before a run |
| `runs/` | saved logs and traces |

One-time setup: `source env.sh; DISPLAY=:77 $WINE wineboot -i` (~20 s).
Build: `i686-w64-mingw32-gcc -O2 -shared -o d3d8.dll hook.c d3d8.def -static-libgcc -Wl,--enable-stdcall-fixup`.

### Probe (`hook.c`)
- **Loading.** The game imports `d3d8.dll!Direct3DCreate8`. The override loads our copy from the game dir. Our copy loads the real d3d8 from `GetSystemDirectory()` and forwards to it.
- **Inline hooks** (all three prologues are `push ebp; mov ebp,esp; sub esp,imm8`, so 6 bytes are stolen). Each stub does `pushad; pushfd; call cb; popfd; popad; <stolen bytes>; jmp back`. The callbacks must not use the FPU.
  - `Rng_Next16` 0x43ECC0: counts calls; logs the CW of any non-main thread that calls it.
  - `Sound_Update` 0x45D790: this is the first call after `FuncChain_RunCalc` in Frame_Run, so it is the post-calc sample point. It writes one trace row per frame while scene = 2 and demo flag set (`traceall` command: every game frame).
  - `GameManager_StageStart` 0x43ABD7: logs thread id and CW.
  - `FuncChain_RunCalc` pass start 0x43CA65 (`mov [ebp-8],0`, 7 bytes): when the chain restarts (return 6, replay fast-forward), emits the row of the previous pass. Without it, fast-forwarded frames are lost, because up to 3 logic frames run per Frame_Run.
- **Vtable hooks.**
  - IDirect3D8::CreateDevice (idx 15): logs the CW before and after, and the present parameters.
  - Device Present (idx 15): logs CW changes, runs commands, takes screenshots of the back buffer before the real Present. The game already sets LOCKABLE_BACKBUFFER. The format is X8R8G8B8 640×480. The output is `shots/<name>.ppm`.
  - Device Reset (idx 14).
- **IAT hook** on `GetKeyboardState`, for key injection. The keyboard normally comes from DirectInput (device ptr 0x17CE768). The `nodik` command zeroes that pointer, which makes `Input_Poll` fall back to GetKeyboardState.
- **Commands** (`probe_cmd.txt`, one per line, `<cond> <action> [args]`):
  - conditions:
    - `pN` = N-th Present since boot;
    - `dK:F` = attract demo K at gameplay frame F (GM+0x3DDC0);
    - `sS:N` = N-th Present since scene S was entered.
  - actions:
    - `shot name`;
    - `key VKhex frames`;
    - `poke1|poke2|poke4 addrhex valhex`;
    - `nodik`;
    - `effscan N` (log active effects of types 4–12 and 59 with slot, age, alpha and ip, for N Presents);
    - `quit`.
  - `traceall` alone on a line enables tracing outside demos.
- **Outputs** (in game/):
  - `probe_log.txt`: events, CW changes, scene changes, commands;
  - `probe_trace.txt`: raw rows; column list in `re/tools/trace2demo.py`;
  - `shots/`.
  `re/tools/trace2demo.py raw outdir` splits the raw trace into the per-demo files in specs/data.

### Timings
- Boot to title: about 2 s.
- With no input, the attract demo starts after about 1550 frames on the title.
- All 4 demos run in sequence in about 500 s: `run.sh 560`.
- Runs are deterministic. Three runs with different boot seeds gave identical demo traces.
- Command scripts used in phase 8 are in `re/work/cmds/`:
  - `spell54.txt` (Spell Practice → 4A → card 54, hold Shot, shots per 15 frames);
  - `ending.txt` (same, then next scene = 9).
- Menu navigation: one 3-frame key press = one cursor step. The locked "Extra Start" is skipped. Spell Practice
  goes straight to the stage-row screen. The card list cursor starts on the first card of the row.
- Probe pitfalls:
  - callbacks must not use the FPU;
  - the loader thread runs StageStart concurrently with Frame_Run, so per-phase RNG counts must be split by thread (the probe counts main-thread calls separately);
  - anm_dis.txt numbers scripts per entry; the runtime script index is global across entries;
  - DI keyboard pointer must be re-zeroed after it is created (`nodik` does it every Present);
  - spell practice does not reseed the RNG, so its bullets differ between runs.

## 4b. `tools/anmspr.py` (bugs2)
`anmspr.py <file.anm> [lo hi]` prints sprite rects (entry, global sprite id, x, y, w, h). Use with `anm.py --png outdir` to look at the sprites.

## 5. `tools/dispscan.py` (phase 9)
`dispscan.py <disp|abs-addr>...` lists every game-code instruction (functions from `callgraph.tsv` below
0x476CE0, capstone) whose **memory operand** has that displacement, e.g. `dispscan.py 0x3d` → all
`[reg+0x3D]` accesses, tagged with the function name. Use it for per-object fields (struct members reached
through a register), which `structxref.py` cannot see (absolute addresses only). Limits: `add reg, imm`
address arithmetic (Ghidra's `(int)this + 0x1b88` passed as a pointer) is not a memory operand and is missed
— grep `all_decomp.c` for `0x1b88,` / `0x1b88)` as well.

## 6. Draw log / autocmp (AUTOCMP brief, 2026-10-18)

Probe sources are now tracked in `re/probe/`: `hook.c`, `drawlog.inc`, `d3d8.def`, `run.sh`, `env.sh`,
`menu_gen.py`, `cmds/`. Copy them to `re/work/hook/` (sources) and `re/work/` (scripts) and build there, as in §4.
Format: `specs/data/drawlog/FORMAT.md`. Data and menu scripts: `specs/data/drawlog/README.md`.

### What the probe adds
- **Device vtable hooks** (on top of §4): SetRenderState 50, SetTextureStageState 63, SetTexture 61,
  SetTransform 37, SetViewport 40, SetVertexShader 76 (FVF), SetStreamSource 83, DrawPrimitive 70,
  DrawPrimitiveUP 72, DrawIndexedPrimitive 71 (only flagged, never used by the game), CopyRects 28, Clear 36.
  After CreateDevice and every Reset, the shadow state is re-read with the Get* methods.
- **Labels.**
  - Inline hooks on `Anm_Preload` 0x465860 and `Anm_LoadSurface` 0x4667C0 record the file name per ANM slot and
    per surface slot. The new `inline_hook_sp` passes the saved registers and stack to the callback.
  - A texture pointer is resolved at draw time by scanning the AnmManager (pointer at 0x18BDC90) slot table
    (+0x24 + slot·0x1C: data, entry count, …, +0x14 texture records of 0xC bytes).
  - Surfaces are resolved through AnmManager +0x2038/+0x20B8.
- **Vertices.**
  - XYZRHW vertices are logged as given.
  - XYZ vertices are projected with the shadow world·view·projection and viewport, in double precision.
  - The texture matrix (COUNT2, D3DTS_TEXTURE0) is applied only to XYZ vertices, the same rule as wined3d for
    pre-transformed vertices.
  - The unit-quad vertex buffer of `Draw3DWorld` is read from its mirror at AnmManager+0x24D0.
  - Colour = diffuse, with the TFACTOR folded in when COLORARG2/ALPHAARG2 = TFACTOR.
- **Records** are buffered per frame and written at Present, after `scene_track()`, so the F line's
  scene/scene_frame is the current one.
- **Commands** (probe_cmd.txt):
  - `dl 0|1|2`: draw log off / every frame / demo frames only. The output is `probe_draw.txt`.
  - `peek addr len`: hex dump to the log.
  - New condition `mN` = N-th frame since the first main-menu frame. This is "script frame N" of the input
    script.
- **Input script** `probe_input.txt`, in `TH08_INPUT` syntax (`frames:mask`, comma/newline separated, `#`
  comments). Script frame k = k-th frame from the first main-menu frame. Keys are injected through
  GetKeyboardState, and `nodik` is turned on automatically. Bit → VK: 1 Z, 2 X, 4 Shift, 8 Esc, 0x10 Up, 0x20 Down,
  0x40 Left, 0x80 Right, 0x100 Ctrl, 0x200 Q, 0x400 S, 0x800 P, 0x1000 Enter, 0x2000 D, 0x4000 R
  (`Input_Poll` 0x43D970 table).

### Regenerating the reference logs
- **Demos.** game dir with `score.dat` (any) and `th08.cfg` as in §4. `probe_cmd.txt` = `p1 dl 2`, the shot lines,
  `p30900 quit`. Run `run.sh 600` (~9 min), which writes ~1.1 GB of text. Then run
  `python3 -I re/tools/drawlog.py split game/probe_draw.txt outdir --chunk 1500` (~10 min, xz -9e) and
  `drawlog.py summary "outdir/demo0_*.drawlog.xz"`.
- **Menus.** Move `score.dat` out of the game dir (fresh state). Then:
  `python3 re/probe/menu_gen.py base|clr outdir`, copy `menu_<v>.input.txt` → `game/probe_input.txt` and
  `menu_<v>.probe_cmd.txt` → `game/probe_cmd.txt`, `run.sh 120` (~45 s), and xz the `probe_draw.txt`.
  Screenshots go to `game/shots/<v>_m_*.ppm`.
  - Shots are taken 5 frames before the next press. A shot taken on the press frame shows the cursor already
    moved.
  - Restore `score.dat` afterwards (kept in `re/work/runs/autocmp/score.dat.user`).

### Validation
`drawlog.py render log P out.png --from P0` rasterises the log with the extracted textures
(`anm.py --png re/work/tex`, images from `re/extracted/dat`).
- Menu char select (present 628): mean abs difference 0.009/255 against the screenshot, 0.01 % of pixels off by
  more than 16. Pixel-exact apart from filtering.
- Demo frames (6 screenshots, d0 f1400/f2200, d1/d2/d3 f1000, d3 f4000) with `--fog`: mean abs difference
  0.20–0.40/255 on the playfield, 0.06–0.24 % of pixels off by more than 16. 3D quads are clipped to 0 ≤ z ≤ w
  (default; `--noclip` turns it off). Fog = per-pixel clamp of the perspective-interpolated eye depth,
  derived from z, not from the 4-decimal rhw (06 §8, answers_impl13 §4).
- `drawlog.py diff` of a log with itself gives 0 differences. Two original runs agree except as listed in the
  data README (fps counter, title intro ±1 frame).

### Facts learned while building it (VERIFIED at runtime)
- The windowed swap effect is COPY (3). The back buffer persists between frames.
  - Every gameplay frame clears depth on the whole screen (`C z`, V 0 0 640 480) and colour+depth only in the playfield (`C tz`, V 32 16 384 448).
  - The side panel is repainted only by `Hud_DrawPanel` full repaints (12 §5.3, answers_impl12 §9).
  - During stage loading, the loading screen copies the screen into the `th08logo.jpg` surface slot
    (`K screen th08logo.jpg`) and pastes it back every frame.
- Game-wide draw paths seen: batched TRIANGLELIST quads (FVF 0x144, 6 vertices per quad, order 0 1 2 1 2 3),
  strip quads (`Draw3DWorld` FVF 0x102 via the VB, or 0x142 UP), ring strips (0x144 strip), untextured 0x44
  quads, CopyRects backgrounds.

## 7. RNG call-site and bullet logging (RE-Q14, 2026-10-18)

`hook.c` (copy in `re/probe/`) has two new `probe_cmd.txt` lines (windows in trace frames, `GM+0x3DDC0`):
- `rngsite K F0 F1`: for attract demo K, frames F0..F1, every main-thread `Rng_Next16` call is written to
  `probe_rng.txt` as `<global call no> <ret> <ebp-chain return addresses…>` (the RNG hook is now
  `inline_hook_sp`, so sp[9] = return address and sp[3] = caller EBP). After each trace row the probe writes
  `R demo K frame F ctr N rng S bullets B`. The draws listed before an `R` line belong to that frame.
  `re/tools/rngsites.py probe_rng.txt [K] [-o]` symbolises the chains with callgraph.tsv (grouped, or in call
  order with `-o`). Frame-pointer-less frames end the chain early; that is harmless.
- `bscan K F0 F1`: dumps every non-free bullet slot per frame (`probe_bullets.txt`: slot, state, pos, vel,
  angle, speed, off-screen counter, grace, ex-active bits, flags, colour, sprite w/h). The timer fields in that
  dump are wrong offsets; ignore them.
Example: `re/probe/cmds/impl14_rng.txt`; the run output is kept in `re/work/runs/impl14/`. One 4-demo run takes
`run.sh 600`.

## 8. Effect slot dump (RE-Q15, 2026-10-18)

`hook.c` (copy in `re/probe/`) has a new `probe_cmd.txt` line `effdump K F0 F1 TYPE` (TYPE −1 = all types): for attract
demo K, trace frames F0..F1, every live effect slot (EffectMgr 0x4ECE60, 653 slots × 0x360) is written after the calc
to `probe_eff.txt`, raw hex. The `F` line holds the RNG state, ring index, stage camera / facing / normalised facing /
right vectors, the overlay colour 0x4E4A64 and the player position. `E` lines hold the slot, type, layer, VM time, world
pos, v, a, VM pos, rotation, scale, colour, colour2, flags, I0/I1, F0, the draw-callback displacement (+0x244), the
speed (+0x238) and pos_offset. Decode with `python3 -I re/tools/effdump.py probe_eff.txt`. Example:
`re/probe/cmds/q15_eff.txt` (`run.sh 420`). The probe also logs the caller of each of the first 12 Presents.

## 9. Session recorder (SESSION brief, 2026-10-18)

- `re/tools/record_session.sh [--score F] [--no-drawlog] [--sound on|off|keep] [--display D] [--chunk N] [name]` —
  user-facing, real display, DEFAULT PortProton prefix, game copy `re/work/game`, output `sessions/<name>/`.
- `re/tools/play_session.sh <name> [--display D] [--drawlog|--no-drawlog] [--sound keep|off] [--out DIR]` — replays the
  session in the original (Xvfb :77 by default, null audio sink), then `session.py finalize` + `compare`.
- `re/tools/session.py finalize <dir>` (polls → input.txt, meta, xz) / `compare <rec> <play>` (trace rows aligned at
  the first menu row, loader columns skipped on loading rows; draw logs via `drawlog.py diff --key scene`).
- `re/tools/xkeys.py <display> <script>` — XTest key events (`wait S`, `press KEY [HOLD]`, `down/up KEY`) for
  scripted test sessions on Xvfb; goes through wine → DirectInput like a real keyboard. Example scripts:
  `re/work/sesstest/keys1.txt` (title → stage 1 → quit, ~30 s), `keys2.txt` (~3 min).
- Probe session mode (`re/probe/session.inc`): replaces `Input_Poll` and `Input_PollJoystickRaw`; hooks boot seed,
  Frame_Run pre/post calc, calc-pass start, MainMenu_Calc / GameMgr_Calc / Result_Calc entries, Rng_SetState and the
  loader park points (see NOTES "SESSION"). Writes polls.txt, events.txt, trace.txt, draw_NNNNN.txt chunks
  (compressed by the recorder script in the background with `xz -3`, re-compressed `-6` at the end).
- Renderer: the prefix's d3d8 is DXVK; the override `c:\windows\syswow64\d3d8.dll=b` makes the probe's
  `LoadLibrary(<sysdir>\d3d8.dll)` load wine's builtin wined3d instead (CW after CreateDevice 0x007F as before).
- Timings: 60.0 fps with the draw log under Xvfb/llvmpipe; draw log ≈ 10 MB/min of gameplay compressed; replay =
  real time; `drawlog.py diff` of a 10.7k-frame session ≈ 30 s.
- Pitfalls found: `pactl load-module` with an existing sink name fails silently (audio went to the default device
  in two test replays); `md5sum` with no files reads stdin (fixed: an empty replay folder made the recorder wait);
  playback must not block the main thread while a loader needs main-thread frames (post-load servicing added).
