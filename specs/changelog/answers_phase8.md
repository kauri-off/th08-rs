# Answers, phase 8 — runtime pilot on the original (wine)

All facts below were measured on the original `th08.exe` (v1.00d, md5 77b6785e…) running under wine
(WINE_LG_11-1, wined3d) with a probe DLL. The procedure is in `re/TOOLING_REPORT.md` §4.
VERIFIED here means "measured at runtime, and consistent with the static code".

## 1. x87 control word — VERIFIED

| point | thread | CW | meaning |
|---|---|---|---|
| DLL load, `Direct3DCreate8`, just before `CreateDevice` | main | 0x027F | 53-bit precision, round to nearest, exceptions masked (CRT default) |
| right after `IDirect3D8::CreateDevice` (behaviour flags 0x40, no FPU_PRESERVE) | main | **0x007F** | **24-bit precision**, round to nearest, exceptions masked |
| every Frame_Run: after the calc chain, and at Present | main | 0x007F | unchanged for the whole run (menus, gameplay, ending) |
| `GameManager_StageStart` (every stage, all 5 observed) | loader thread | **0x027F** | 53-bit precision |
| `Rng_Next16` calls made from the loader thread | loader thread | 0x027F | |

Consequences (now in 05 §10 and 11 §1.2):
- Main thread: every x87 arithmetic result is rounded to a float32 mantissa, one rounding per operation, including
  intermediates kept in registers. Implement gameplay math as plain f32 operations in source order.
  - `Rng_Float` = `(n as f32) / 2^32` exactly. The integer load is exact; the single division rounds once.
- No `fldcw` exists in game code (0x402000..0x476CE0). The only ones are in CRT and D3DX helpers, which restore
  the CW. So the CW never changes after CreateDevice on the main thread.
- Loader threads keep the CRT default. Float math in StageStart (and in the menu loader) that chains operations in
  registers is computed at 53 bits. Single operations stored straight to f32 give identical results.
- The draw phase consumed **no** main-thread RNG in any run (a later probe version counts per thread: the only "between calc and Present" draws seen came from the loader thread running StageStart concurrently, e.g. 8 draws at a spell-practice start). The probe compares the call counter after the calc chain with the
  counter at Present. So the RNG sequence is defined by the calc chain alone.
- Caveat: the measurement is wine's implementation of the D3D8 "no FPU_PRESERVE" behaviour, which sets the FPU to
  single precision as documented for Windows. The trace in §2 is internally consistent with it. The RNG and replay
  sync were identical across runs.

## 2. Ground-truth traces of the 4 attract demos — VERIFIED

Files: `specs/data/demo0_trace.txt` … `demo3_trace.txt`.
- Each file has one row per logic frame: every frame of the demo, with no gaps, from gameplay frame 1 until the
  demo returns to the title.
- Source: the attract demos started from the title with no input (demo index 0→3 in order, about 1550 title frames
  between them). The demos are not loaded through the replay menu.

| file | replay | stage | shot | frames | spell rows | fast-forward rows |
|---|---|---|---|---|---|---|
| demo0_trace.txt | demorpy0.rpy | 5 | 0 | 6120 | 0 | 0 |
| demo1_trace.txt | demorpy1.rpy | 4A | 1 | 4920 | 0 | 0 |
| demo2_trace.txt | demorpy2.rpy | 3 | 3 | 5040 | 654 | 207 |
| demo3_trace.txt | demorpy3.rpy | 2 | 2 | 7020 | 567 | 26 |

The frame counts equal the demo time limits of 11 §4 (6120 / 4920 / 5040 / 7020).

### 2.1 Format
- Text, space-separated.
- Three `#` header lines:
  1. demo, replay file and stage;
  2. the RNG state just **before** frame 1, and the number of loading frames;
  3. the column names.
- Each row is the state **after** that frame's calc chain. The sample point is the entry of `Sound_Update`, which
  Frame_Run calls right after `FuncChain_RunCalc`. When the chain is restarted (return 6, replay fast-forward), the
  sample is taken at the start of each new pass instead.
- Floats are given as the hex of their IEEE-754 bits, so they compare bit-exactly.

| col | name | source | format |
|---|---|---|---|
| 1 | frame | GM+0x3DDC0 gameplay frame counter | dec |
| 2 | demo_frame | GM+0x3DBB8 demo frame counter (+1 every logic frame; the key column) | dec |
| 3 | input | 0x164D52C game input word for this frame (replay-substituted) | hex u16 |
| 4 | rng | g_Rng state 0x164D520 | hex u16 |
| 5 | rng_ctr | Next16 calls since the state in header line 2 (cumulative) | dec |
| 6, 7 | px, py | player pos 0x17D61AC / 0x17D61B0 | f32 bits |
| 8 | pstate | player state byte 0x17D5EF8 (0 normal, 1 respawning, 2 dying, 3 invulnerable; 09 §2) | dec |
| 9 | score | stats+0x08 (score ÷ 10) | dec |
| 10 | graze | stats+0x0C graze total | dec |
| 11 | points | stats+0x30 point items total | dec |
| 12–14 | lives, bombs, power | stats+0x74 / +0x80 / +0x98 | f32 bits |
| 15 | gauge | stats+0x22 human/youkai gauge | dec i16 |
| 16 | time | stats+0x3C time this stage | dec |
| 17 | bullets | live bullet count 0x160F3C8 (from the last BulletMgr calc) | dec |
| 18 | enemies | number of enemy slots with alive bit (e+0x3324 bit 0), 480 slots at 0x577F20+0x53D0 | dec |
| 19 | rank | GM+0x3DE2C | dec |
| 20 | spell | g_Gui (0x4EA670) bit 0, spell card active | 0/1 |
| 21 | boss | bitmask of non-null boss pointers 0xF54CC0[0..3] | hex |
| 22 | flags | GM flags 0x164D0B4 | hex |
| 23 | ff | index of the calc-chain pass inside one Frame_Run. 0 = normal; 1, 2 = extra passes while the replay fast-forwards (input bit 0x100, 15 §3.3; up to 3 logic frames per Frame_Run) | dec |

### 2.2 Facts confirmed by the traces
- **Seed → first frame.** For all four demos, the state before frame 1 = the replay's stage seed advanced by
  exactly **2** Next16 draws:
  - 8d07→713f, 684d→351e, d39a→c872, 0322→0de7;
  - the two draws are the `EnemyManager_Load` Range16 calls after the seed capture (11 §1.3/§1.5).
- **Loading.** 81 calc frames pass in the loading state (GM+0x38 = 1) before gameplay frame 1 in every demo. That
  is the replay value 80 of 11 §3, plus the frame that clears it.
- **Title menu.** Idling on the title consumed **0** RNG draws: the counter at StageStart equals the counter at the
  end of the previous demo.
- **Determinism.**
  - Three independent runs with different boot seeds (timeGetTime) gave identical rows for every frame (22 945 frames compared).
  - So nothing outside the replay stream affects the demos: not timing, not frame rate, not the boot seed.
- During fast-forward (input 0x100) the original runs up to 3 calc passes per Frame_Run. The port must do the same
  to stay in sync, and must step its frame counters per pass.
- Frame-1 rows show player (192, 384), lives 8, bombs 3, power 128 (demo3: power 112, as in its record).

### 2.3 Using it for a regression test
1. Load `demorpyN.rpy` into the port as an attract demo:
   - shot, difficulty and stage come from the replay;
   - GM+0x3DBAA = shot % 2;
   - demo flag set.
2. Run the stage setup, then check that the RNG state equals header line 2.
3. Step one logic frame per row. Compare at least `rng`, `rng_ctr`, `px`, `py`, `score`, `lives`, `bombs`, `power`.
   The first mismatching frame localises the divergence.
   - `bullets` and `enemies` help to tell an RNG bug from a spawn bug;
   - `input` checks the replay reader, including the 1-frame latency and the key-repeat emulation.

## 3. Reference screenshots — `specs/data/ref/`

All are back-buffer captures (640×480 X8R8G8B8 → PNG), taken just before `Present`. The game ran windowed with the
default config. Effects level was 2 (cfg+0x24). Music and SE were off; they do not change any picture.

### 3.1 Spell practice, card 54 (`夢符「二重結界」`, Easy, Reimu, stage 4A, shot 0 = Reimu & Yukari)
- Reached through the real menus: Spell Practice → stage row 3 → first card.
  - To unlock the card, story-attempts[54] (0x160F69C + 54·0x22C) was poked to 1.
  - The player stays at the spawn position and holds Shot from the start.
- Frame numbers are **gameplay frames (GM+0x3DDC0)** of that run. The boss's ECL sub28 timeline starts the card at
  gameplay frame ≈ 0 of the card. Native 3 runs at sub time 90.
- The RNG was not re-seeded in spell practice, so bullet positions vary between runs. The overall timeline and the
  pictures are stable: a second run was visually identical.

| file | frame | shows |
|---|---|---|
| spell54_ring_f059.png | 59 | spell-declare ring (large wavy ring) around the boss position, before the boss appears |
| spell54_ring_f074.png | 74 | ring collapsing into the starburst |
| spell54_burst_f089.png | 89 | burst at the boss, boss arriving at (192, 208) |
| spell54_cutin_f134.png | 134 | portrait cut-in, spell background, card name |
| spell54_boss_under_fire_f209.png | 209 | boss under fire: player shots hitting Reimu, score rising, boss life bar |
| spell54_natives3_mask_f269.png | 269 | **natives-3 mask**: square frames (effect 9/10 rings inscribed squares) + mirrored spell background, first bullets |
| spell54_natives3_mask_f389.png | 389 | natives-3 mask with bullets crossing the barrier; boss under fire |
| spell54_natives3_mask_f479.png | 479 | same, later, dense |
| spell54_capture_flash_f539.png | 539 | card captured: white flash, bonus text |

### 3.2 Ending fade (`end00a.end`, bad ending, Reimu & Yukari)
- Reached from the same spell-practice game by poking Supervisor next-scene 0x17CE8B4 = 9 at scene-2 present 300.
  `Ending_Init` chose `end00a.end`, which matches 16 §1.2 (shot 0, bad ending).
- The background is `end00.jpg`: the downscaled capture matches it with a mean abs diff of 0.6/255.
- Frame numbers = Presents since scene 9 was entered (`s`).
- Script: `@2 30000` (white overlay with alpha 255 − t·255/30000), `@w 120 120`, then `@2 180` (fade from white over
  180 frames).

| file | s | pixel (20,20) RGB | mean |
|---|---|---|---|
| ending_fade_s0115.png | 115 | 255,255,255 | 255.0 |
| ending_fade_s0125.png | 125 | 254,253,253 | 254.0 |
| ending_fade_s0155.png | 155 | 243,236,237 | 243.5 |
| ending_fade_s0185.png | 185 | 231,217,220 | 232.7 |
| ending_fade_s0215.png | 215 | 220,200,204 | 222.1 |
| ending_fade_s0245.png | 245 | 208,181,187 | 211.4 |
| ending_fade_s0275.png | 275 | 197,164,171 | 200.8 |
| ending_fade_s0305.png | 305 | 186,147,156 | 191.0 |
| ending_fade_s0450.png | 450 | 186,147,156 | 190.9 (fade finished) |

Check against 16 §2 (`@2 n`: overlay ARGB = (255 − t·255/n) << 24 | FFFFFF, normal alpha blend):
- with the 180-frame fade starting at s ≈ 124, s = 215 gives t = 91, alpha = 255 − 128 = 127, and
  186 + (255 − 186)·127/255 = 220.4. Captured: 220. VERIFIED.
- The fade is linear and ends at s ≈ 304.

## 4. Spec changes in this phase

| spec | change |
|---|---|
| 05 §10 | FPU environment VERIFIED at runtime (CW values, no fldcw in game code, loader-thread exception) |
| 11 §1.2 | precision VERIFIED; new paragraph on loader threads (53-bit) |
| COVERAGE.md | floating-point row → VERIFIED |

## 5. Open / uncertain

- The CW values are wine's emulation of D3D8. Native Windows is expected to match: it is the documented D3D
  behaviour, and the replay sync is self-consistent. Native Windows was not available to check.
- Which float expressions in the loader thread actually chain operations in registers was not audited. Use the f64
  rule in 11 §1.2 for stage-start initialisation if a mismatch shows up at frame 1.
- The screenshots are from wined3d (OpenGL). Per-pixel filtering and blending can differ slightly from a Windows D3D8
  driver. Geometry, ordering, colours and fades are reliable.
