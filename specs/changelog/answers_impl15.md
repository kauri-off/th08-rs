# Answers to impl15 questions (docs/briefs/IMPL15_QUESTIONS.md)

Sources: th08.exe static code (`re/extracted/all_decomp.c`, disassembly), and wine probe runs (`re/work/runs/q15`).
New reference data in `specs/data/q15/`:
- `rng_d0-3_f1-4.txt` — RNG call chains, demos 0–3, frames 1–4 (`re/tools/rngsites.py <file> K [-o]`).
- `eff51_raw.txt` and `eff51_decoded.txt` — every type-51 effect slot after the calc of demo 1 and demo 3,
  frames 1–6 (new probe command `effdump K F0 F1 TYPE`, TOOLING_REPORT §8). The decoded file gives world pos,
  velocity, acceleration, VM rotation z, scale, colour, colour2, flags, I0/I1, the draw-callback displacement D,
  the stage camera, the overlay colour and the player position.

Order follows the priorities of the brief. Question numbers are those of IMPL15_QUESTIONS.

---

## Q9. Stage particles in demos 1 and 3 (etama.anm#1, sprite 220) — VERIFIED (runtime + code)

### Spawn source
- Frame 2 of demo 1 (stage 4A) and demo 3 (stage 2): an enemy spawned from the ECL timeline
  (`EnemyMgr_SpawnFromTimeline`) runs its ECL at once. Its sub executes op 139 = `Effect_Spawn(51, pos, n = 16, colour)`.
  Afterwards the same enemy repeats it from `EnemyMgr_Calc`. The trace `rng_ctr` column (demo 1) goes +416 on the
  rows of frames 2, 5, 9, 13, 17 (first gap 3, then 4: the spawn-frame ECL pass counts as the sub's first frame), and
  +520 on frame 21 (the ECL changes n later).
- Demo 2 (stage 3) spawns the same 16 particles on frame 2. Demo 0 (stage 5) spawns none in frames 1–3.
- Each call consumes **416 Next16 draws = 16 effects × 26**.

### RNG order for one effect (VERIFIED by the call-order log and by a float-exact replay)
`Effect_Spawn` allocates the slot. Then `AnmFile_SetScript` (+0x10B) runs script 73 once at t = 0. Then the init
function runs (+0x188). The 26 Next16 draws for one effect, in order:

| # | call | Next16 | use |
|---|---|---|---|
| 1 | `frand 0.05236` (Rng_FloatRange) | 2 | F0 → F0 − 0.02618 = angular velocity z |
| 2 | `frand 0.1` | 2 | F0 + 0.1 = scale x = y; then F0 ·= 0.5 (scale_time target at t = 31) |
| 3 | `irand 32` (Rng_Range) | 2 | I0 + 48 = alpha target of `alpha_time 30` |
| 4 | `irand 127` | 2 | I0 + 128 = colour R (applied by `color` at t = 1) |
| 5 | `irand 127` | 2 | I1 + 128 = colour G (B = 255) |
| 6–13 | `EffInit_Particle51`: S(60), S(100), R(100), S(0.001), S(0.03), R(0.1), S(0.0001), S(0.0001) | 16 | x, y, z, vx, vy, vz, ax, ay (answers_impl9 §4) |

Then the next effect of the same call. `Rng_Range(n)` = next32 % n (2 draws), `Rng_FloatRange` /
`Rng_SignedRange` = next32-based (2 draws), as in 11 §1.1.

Float-exact check: starting from the RNG state after frame 1 of demo 1 (0x351E, trace row 1), the model above
gives I0/I1 and the world positions of all 15 surviving effects. I0/I1 match exactly; positions match to
≤ 1e-4. The remaining difference comes only from the rounded camera value used in the check.

Init expression order (to be bit-exact; the main thread runs at 24-bit precision):
- base = F + C, written into the world position first;
- x = (−F.x / 2 + S(60)) + base.x;
- y = (−F.y / 2 + (S(100) − 50)) + base.y;
- z = (−F.z / 2 + (R(100) − 100)) + base.z;
- vx = S(0.001) + param.x; vy = S(0.03) + param.y; vz = (−R(0.1) − 0.3) + param.z;
- ax = S(0.0001); ay = S(0.0001); az = −0.0003;
- then v and a are multiplied by game speed.

C = camera (0x4EA3C4), F = facing (0x4EA3D0), both the values current at spawn time (ECL runs before
`Stage_Calc` in that frame). Demo 1 frame 2: C = (0, 5.3155, −300), F = (0, 500, 300).

### What happens in the spawn frame (VERIFIED, effdump)
1. The slot is taken from the 512-slot ring. The ring index was 0 at stage start, so the 16 effects go to slots 0–15
   and the ring ends at 16.
2. Script 73 runs once inside `SetScript`. The VM time becomes 1.
3. `EffectMgr_Calc` (later in the same frame) visits slots in index order 0..652. For each live effect:
   - **update fn first**: `EffUpd_Particle51`: v += a, world += v, pos = world, view-cone cull
     (dot < 0.94 → freed). Then colour2 = VM colour × overlay colour (`g_Stage`+0xA34, current value, per byte (a·b) >> 8),
     and VM flag 0x20000 is set.
   - **then `AnmVm_Run`**: the t = 1 `color` op runs in the spawn frame. After the frame the VM time is 2.
   - then the slot is linked into list 1.
   So after the spawn frame, colour = (A = alpha_time progress, R = I0, G = I1, B = 255). colour2 = 0, because
   it was computed from the spawn colour (op 139 colour, 0 here) before the `color` op ran.
4. Demo 1 frame 2: slot 0 is culled by its first update. Slots 1–15 survive. Demo 3: slot 8 is culled.
5. Draw (`EffectMgr_DrawBackground`, list 1 = **slot order**, not spawn order):
   - billboard + `EffBg_ScreenPullCb` (answers_impl9 §8);
   - colour = **colour2** (flag 0x20000), so the spawn-frame draw is 00000000 (invisible);
   - frame 3 draws colour2 = colour-after-frame-2 × overlay-of-frame-3. Example, demo 1 slot 1: 0x05E181FF × 0xFFDBDBFF → **04C06EFE**, which is exactly the logged quad colour.
   - The draw callback accumulates the player push D (|d| < 88). Slots 10 and 15 of demo 1 already have
     D ≠ 0 after the frame-2 draw.
   - 14 of the 15 live particles are drawn at present 9390. Slot 7 is skipped: no quad, so it is rejected by the billboard's s.z ∉ [0, 1] test (INFERRED).
6. The stage-4A overlay colour fades during these frames: 0xFFF3F3FF (f1), FFE7E7FF, FFDBDBFF, FFCFCFFF, …, i.e. −0x0C per
   frame on R and G. The particle colours depend on it.

### Checklist for the port (in order of likelihood)
1. The draw colour must be colour2 (VM colour × current overlay colour), not the VM colour.
2. In `EffectMgr_Calc` the update fn (cull + colour2) runs before `AnmVm_Run`. The VM is run once more in the spawn frame
   (StartScript at t = 0 plus the calc run at t = 1).
3. Within one spawn, all 5 script draws come before the 8 init draws. The script draws are frand, frand, irand, irand, irand.
4. Draw order = slot index order. The ring index persists across spawns and starts at 0 at stage start.
5. The camera C/F used at spawn must be the value before this frame's `Stage_Calc`.
6. The screen-space push D accumulates at draw time only (it is skipped while paused).

`eff51_decoded.txt` gives every one of these values per slot and frame for a numeric diff.

---

## Q6. Black half-transparent overlay on the first 9 gameplay frames — VERIFIED

It is a **ScreenEffect type 5** ("darken", 13 §3), created when a stage starts from the main menu, and **released** when loading ends.
- `GameMgr_OnRegister` 0x43AAF4: if the previous scene (Supervisor+0x160 = 0x17CE8B8) is 1 (main menu,
  includes attract demos), it calls `0x44886F(500, 440)` (loading banner + screen capture, below) and then
  `0x4489CA(0)`. Otherwise it only calls `0x44886F(280, 430)`, with no overlay.
- `0x4489CA(k)`: if the global 0x17CE750 is 0, it calls `ScreenEffect_Register(type 5 + k, p0 = 60, p1 = colour 0,
  p2 = 0, p3 = 0, draw prio 1)` and stores the object in 0x17CE750. `MainMenu_OnRegister` calls it too (k = 0, menu loading).
- Calc (0x45B800, calc prio 3):
  - not released: if p0 ≠ 0 and t ≤ 60: alpha = trunc(t·128/60);
  - released (+0x24): if t ≤ 8: alpha = 128 − trunc(t·128/8), else the effect is removed;
  - the timer then ticks.
- **The calc does not run during stage loading.** `GameMgr_Calc` (prio 2) returns 3 while `loading ≠ 0`, so prio 3 never
  runs. Alpha stays 0 and the draw (prio 1, full screen 640×480, colour = alpha << 24 | p1) logs alpha-0 quads
  on every loading frame.
- Release: `Loading_Wait` 0x448972 (stage loader thread, end of StageStart) or `Loading_End` 0x44881A (menu
  loader) call 0x45C160: +0x24 = 1, timer = 0, and clear 0x17CE750.
- Result: game frame 1 alpha 0x80, then 0x70, 0x60, …, 0x10, 0x00 at frame 9, removed at frame 10. This matches all 4 demo logs
  (presents 9388–9396 of demo 1).
- Draw order: draw prio 1 comes before the playfield clear (prio 2+). The quad is drawn first in the frame, over the
  previous back-buffer contents (FORMAT.md persistence), on the whole screen.

---

## Q7. Gauge readout timing at stage start — VERIFIED mechanism, measured offsets

- The readout VMs are restarted (scripts 5–8) and get the stored interrupt (1 on a fresh start) in
  `Ascii_SetupGaugeVms` near the end of `GameManager_StageStart`, **on the loader thread** (12 §5.9).
- `Ascii_Calc` (calc prio **1**, below `GameMgr_Calc`'s prio 2) keeps running during the loading frames. So the
  label-1 animation advances during loading, starting from whatever loading frame the loader thread reached the setup in.
- That point is a thread race; it depends on how long the stage load took. Measured from the draw logs by fitting the G0 slide
  (y = 465 − 16·k/40, snapped round-to-nearest − 0.5) to every frame:

| demo | stage | label-1 age k at the game-frame-1 draw | slide ends (y 448.5) at gm | 人/妖/▼ first visible (k = 61) at gm |
|---|---|---|---|---|
| 0 | 5 | 28 | 12 | 34 (alpha 0x06) |
| 1 | 4A | 27 | 13 | 35 |
| 2 | 3 | 29 | 11 | 33 |
| 3 | 2 | 33 | 7 | 29 |

  k counts Ascii_Calc ticks since the setup interrupt, including the tick of game frame 1.
- So at frame 1 the bar is still sliding, and the zone markers (G1–G3, alpha 0 until script time 60) are not drawn. Alpha-0 quads are
  not emitted.
- The port shows the markers at alpha 0x85 on frame 1. That means the port sends the interrupt about 81 frames before frame 1
  (at loading start). The port should run the setup at loading frame (81 − k) of the 81 replay loading frames.
  For an exact log match, k comes from the table above. Otherwise use k = 28.
- The player's own interrupt 1 (in `Player_Load`, at the top of StageStart) is overridden by the setup's script restart. Only
  the setup time matters.

---

## Q1. Boot timing — VERIFIED (runtime present-caller log)

- Presents 1 and 2 come from `Window_InitD3D` → 0x45B0E0 → 0x45B020: viewport 640×480, Clear(target+z, black), Present,
  twice. The draw log shows only `C tz` on present 2, because logging starts after present 1.
- Present 3 onwards = `Frame_Present` (0x442060, return address 0x442082). The Supervisor calc runs from the first
  `Frame_Run`. `Loading_Begin` has already run synchronously in `Supervisor_OnRegister`, so state = 1 and the 3 VMs have their scripts.
- **Presents 3, 4 and 5 contain no primitives at all** (no CopyRects of surface 8, no VM quads). This holds in all 3 menu
  logs. Present 6 is the first logo frame. By then the nowloading VMs are at script time 5 (alpha 0x9F = 255·5/8 for
  script 0, 0x18 = 96·5/20 for the ring). So the VMs ticked on every calc from present 3 on; only drawing was absent.
  Cause: INFERRED to be the loader thread (`Supervisor_Init`: RNG seed, MIDI, SE, `text.anm`) holding the
  device / critical section during its first frames. The code has no explicit "skip draw" counter.
- Port rule: present 2 black frames, then 3 frames with VM ticks but no drawing. From the 4th Frame_Run frame on, draw
  surface 8 + VMs. The nowloading VMs are then one frame behind the port's current timing (the port draws from present 1).
  There is **no** fixed delay before `Loading_Begin`.

## Q2. Loading VMs draw (0x445D3E) — VERIFIED

Supervisor draw node at prio 2. For the 3 VMs at 0x17CEAC0 + i·0x2A4, i = 0..2:
- pos += pos_offset (+0x288);
- `AnmManager_Draw2DRot`;
- pos −= pos_offset.
Draw2DRot = mode 2: snapped like mode 0 when rotation z = 0, rotated otherwise (05 §8). The anchor (500, 440) or (280, 430) goes into
pos (`Loading_Begin` / 0x44886F). The scripts set `pos_target_offset`, so their `pos` / `pos_time` ops
write pos_offset. Surface 8 (`Anm_DrawSurface`, CopyRects) is drawn by the prio-0 node 0x445BD4 before it.

---

## Q8. HUD z / zwrite — VERIFIED

- z = the VM's pos.z. `Hud_DrawPanel` sets pos with explicit z constants:
  - **0.49**: border tiles (VM +0x2254) at x 0 and x 416..624, the strips at y 0/464 (+0x24F8, also the
    every-frame strip along y 464 for x 32..368), and the hiscore/score row blanks (+0x279C at y 40/56);
  - **0.48**: the blanks of the rows that changed (lives 88, bombs 104, power 136, graze 152, point 168,
    time 184) and the extra blank at the bottom (+0x279C);
  - **0.46**: the life icons (+0x1A68) and bomb icons (+0x1D0C).
- ASCII strings are queued with z = 0. The gauge marker G3 has 0.1 from its script.
- zwrite = **not** VM flag bit 13. Front.anm scripts 0–12 contain `zwrite_off` → zwrite 0. Tile scripts 13–15 (and
  ascii.anm string glyphs) do not → zwrite 1. With ztest = always the depth is written but never tested later in the frame.
  The next frame starts with the full-screen `C z`, so the value has no visible effect. Match it only for log parity.

## Q10. Panel tiles over the playfield edge — VERIFIED (expected)

Yes, this is expected. The original repaints panel tiles only on full repaints / dirty rows (12 §5.3, answers_impl12 §9) and relies on the
persistent back buffer. A port that repaints every frame emits the extra `front.anm#0` quads. Ignore them in
`--playfield` diffs.

## Q11. stgNtxt.anm state — VERIFIED

The `stgNtxt.anm` scripts (e.g. stg5txt scripts 0–3) contain no `blend_mode` and no `zwrite_off`. Every stage-title
quad is drawn with the default VM state: `blend=alpha`, zwrite 1, ztest always, `c=tex*col a=tex*col`. This holds for
the whole of demo 0 (only one distinct S line for stg5txt). The port must not inherit the previous sprite's blend
state or force zwrite 0 for these VMs. Colour changes come only from `color_time` (scripts 0/1 at t = 970 →
(80, 96, 144) over 120 frames) and alpha ops.

## Q12. Menu interrupt rule on screen entry — VERIFIED

- `Menu_SetScreen` (0x4694C3) is called inside the old screen's handler. It stores the old screen in +0x68, sets the new
  one, and zeroes +0x10, +0x1442C (frames since entry), +0x0C (sub-state) and +0xC294. The handler then returns
  at once.
- `MainMenu_Calc` dispatches on the screen value. So the **new** handler first runs on the next calc frame.
- All 9 handlers (Top, Option, KeyConfig, Difficulty, Character, Replay, PracticeStage, SpellPracticeChar,
  SpellPracticeCard) start with an entry block guarded by `+0x1442C == 0`. This block sends the range interrupts and sets the cursor
  sprites/interrupts (e.g. Difficulty: range interrupt 7, VMs 131+i interrupt 25, selected 24).
- `AnmManager_RunRange` runs after the handler in the same `MainMenu_Calc`. So the interrupts take effect in the entry
  frame's VM run.
- Rule: press on frame P → SetScreen on P → entry block + interrupts + VM run on P + 1. The counter +0x1442C is
  incremented at the end of every handler call.

## Q3–Q5. Menu lists, replay screen, result viewer, music room — pointers

- Q3 practice stage select: 12 §2.5 (draw, MOPUP-1). The portrait/VM visibility of the other teams follows the
  character-select interrupts (12 §2.4).
- Q4 replay screen 7: 12 §2.8 (list/detail layout anchored to VMs 78/79/80+k/95+j/97).
- Q5 result viewer: 16 §2.3–2.4 (state machine, menus, X = cursor to 「タイトルに戻る」 then leave), 2.5–2.7. Music room:
  16 §5. **New (VERIFIED):** leaving the music room calls 0x44886F. Besides `Loading_Begin` it queues a post-present
  capture: AnmManager+8 = 8, source and destination rect (0, 0, 640, 480). If no capture is pending,
  `AnmManager_ApplyPendingCaptures` (0x443B60, after Present) copies the back buffer into surface slot 8
  (0x467180: release the old slot-8 surfaces, create 640×480, copy). From the next frame `Supervisor_DrawLoading` CopyRects that
  surface to the screen. That is the `K screen th08logo.jpg` record: the slot keeps the logo's file name in the probe, but its
  contents are the last menu/music-room frame. The same call is used by `GameMgr_OnRegister` (every stage load) and
  the menu-loading paths.

---

## Files changed
- 13 §2.6 (type 51 spawn-frame order / colour2 / slot order), 13 §3 (type 5 loading darken).
- 12 §1 (boot presents, 0x445D3E, overlay, capture), 12 §2 (entry rule), 12 §5.3 (z/zwrite), 12 §5.9 (stage-start timing).
- 16 §5.3 (music-room exit capture). COVERAGE post-present capture row.
