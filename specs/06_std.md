# 06 — Stage background (.std) and the 3D stage camera

Source: th08.exe 1.00d — `Stage_LoadStd` 0x409CE0, `Stage_Calc` 0x407400 (calc prio 8),
`Stage_DrawLow` 0x409200 (draw prio 6), `Stage_DrawHigh` 0x409640 (draw prio 7), `Stage_DrawLayer`
0x40A1B0, `Stage_UpdateObjects` 0x409F40, `Stage_SetCamera` 0x40B6D0. The stage object is a global
(`g_Stage` 0x4E4030). Tool: `tools/stddump.py` parses and dumps all 18 `.std` files (header, objects, quads,
instances, script); over all files: every opcode known, every jump target valid, quad counts match the header,
0 problems. ANM details (VM, draw functions) are in 05_anm_vm.md.

**VERIFIED** = read from code and consistent with data; **INFERRED** = interpretation.

## 1. File layout — VERIFIED

All values little-endian. Offsets are from file start.

| off | type | meaning |
|---|---|---|
| 0x000 | i16 | object count |
| 0x002 | i16 | total quad count (= number of ANM VMs allocated, "bgscroll" array) |
| 0x004 | u32 | offset of the instance list |
| 0x008 | u32 | offset of the script |
| 0x00C | u32 | 0 (unused) |
| 0x010 | char[5][0x80] | text fields ("dm", "dm", "dm", " ", " " in all files); never read by the code |
| 0x290 | char[4][0x80] | BGM file names for music slots 0..3 (" " = none). Slot 0 always played at stage start, slots 1/2 if not " "; other slots by ECL/MSG music-change (later phase) |
| 0x490 | u32[count] | offsets of the objects |

The loader converts the object offsets to pointers (only when the file is freshly loaded).

### 1.1 Object

| off | type | meaning |
|---|---|---|
| 0x00 | i16 | object id (= its index in all data) |
| 0x02 | u8 | layer 0..3 (data uses 0, 1, 2) |
| 0x03 | u8 | runtime flags (0 in file): bit 0 = "has a running script" (set at load, cleared when all its quad VMs finished), bit 1 = drawn this frame |
| 0x04 | f32[3] | object origin x, y, z |
| 0x10 | f32[3] | object size (bounding box extent) x, y, z |
| 0x1C | quads | variable-size records until a record with type < 0 |

### 1.2 Quad records (start with i16 type, i16 size)

| type | size | layout |
|---|---|---|
| 0 (sprite quad) | 0x1C | +4 i16 ANM script index (in the stage's `stgNbg.anm`), +6 i16 VM index (written at load), +8 f32[3] position (relative to object origin), +0x14 f32 width, +0x18 f32 height (0 = keep sprite size) |
| 1 (ribbon) | 0x24 | +4 i16 script, +6 i16 VM index, +8 f32[3] point A, +0x14 f32[3] point B, +0x20 f32 width (0 = sprite width) |
| −1 | 4 | terminator |

Data: 384 type-0 quads, 168 type-1 quads.

### 1.3 Instance list (16-byte records)

| off | type | meaning |
|---|---|---|
| 0x0 | i16 | object index (< 0 terminates the list) |
| 0x2 | i16 | 256 in all data; never read |
| 0x4 | f32[3] | instance position (added to object origin and quad positions) |

Objects are drawn once per instance, in instance-list order.

### 1.4 Script (20-byte records)

| off | type | meaning |
|---|---|---|
| 0x0 | i32 | time (frame); −1 terminates |
| 0x4 | i16 | opcode |
| 0x6 | i16 | arg size = 12 |
| 0x8 | 3 × 4 bytes | args (int, float or D3DCOLOR depending on opcode) |

After the terminator 16 of 18 files contain another 20 bytes of 0xFF (ignored).

## 2. Loading — VERIFIED

`Stage_Load` (on-register callback of the stage calc node):
1. Script clock (Timer at +0x80C) = 0; instruction index (+0x818) = 0; origin (+0x824) = (0,0,0).
2. Load `stgNbg.anm` into ANM slot 4 (04_file_index.md) and `stageN.std` (or `stageN_s.std` in the
   alternative mode, 04_file_index.md).
3. Allocate `quad_count` ANM VMs. For every object (in order) set flag bit 0 = 1; for every quad in order
   assign the next VM index `k`, store k into the quad's +6 field and start the quad's ANM script on VM k
   (`AnmManager_SetScript`, which zeroes pos/pos_offset and runs the script's time-0 instructions).
4. Start the stage overlay VM (+0x844) with script 33 of a global ANM file (0x17CE8F4) and send it interrupt 2
   (VERIFIED in Phase 3: text.anm, human/youkai stage tint — §13).
5. Defaults: fog colour 0xFF000000, fog near 200, far 500; camera position (0, 0, 1000), facing (0, 0, 0),
   eye offset (0, 0, 0), up (0, 1, 0), fov 0.5235988 (π/6); the "target" and "previous" camera blocks are
   copied from these values; no interpolation active; sway off.
6. Cull distance² (+0x6470): 1 322 500 (= 1150²); stage index 5: 1 822 500 (1350²); stage index 6 or 7:
   3 240 000 (1800²).

## 3. Stage state (partial layout of `g_Stage`) — VERIFIED

| off | type | meaning |
|---|---|---|
| 0x000 | ptr | VM array (one AnmVm per quad) |
| 0x004 / 0x2A8 / 0x54C | AnmVm | background VMs A / B / C (ops 29 / 30 / 34) |
| 0x7F0 | ptr | `stgNbg.anm` file |
| 0x7F4 | ptr | std file data |
| 0x800 / 0x804 / 0x808 | ptr | object table / instance list / script |
| 0x80C | Timer | script clock |
| 0x818 | i32 | script instruction index |
| 0x81C | i32 | frame counter (since stage start) |
| 0x824 | f32[3] | origin (op 0), subtracted from every world position |
| 0x830 | u32 | clear colour (op 13; overwritten every frame by VM C's colour while VM C runs) |
| 0x844 | AnmVm | overlay/tint VM |
| 0xAEC / 0xAF0 | f32 | fog near / far (current) |
| 0xAF4 | u32 | fog colour (current, D3DCOLOR) |
| 0xAF8, 0xAFC, 0xB00 | f32, f32, u32 | fog start values (near, far, colour) for interpolation |
| 0xB04, 0xB08, 0xB0C | f32, f32, u32 | fog target values |
| 0xB10 | i32 | fog interpolation duration (0 = inactive) |
| 0xB14 | Timer | fog interpolation elapsed |
| 0x6260 | i32 | section request (written by ECL opcode 147 = 0x93; 0 = none) |
| 0x6264.. | block | camera **target** values: pos 0x6264, facing 0x6270, up 0x627C, fov 0x62AC |
| 0x62B0.. | block | camera **start** values: pos 0x62B0, facing 0x62BC, up 0x62C8, fov 0x62F8 |
| 0x62FC / 0x6308 / 0x6314 | f32[3] | end tangents (pos / facing / up) for Hermite mode |
| 0x6348 / 0x6354 / 0x6360 | f32[3] | start tangents (pos / facing / up) |
| 0x6394 | f32[3] | camera position (current) |
| 0x63A0 | f32[3] | camera facing vector (look-at point = position + facing) |
| 0x63AC | f32[3] | camera up vector |
| 0x63B8 | f32[3] | normalised facing (recomputed every frame) |
| 0x63D0 | f32[3] | eye offset (op 32 and sway): eye = position + eye offset |
| 0x63DC | f32 | vertical field of view (radians) |
| 0x63E0 | i32[4] | interpolation durations: 0 pos, 1 facing, 2 up, 3 fov (0 = inactive) |
| 0x63F4 | Timer[4] | interpolation elapsed timers |
| 0x6430 | i32[4] | interpolation modes |
| 0x6424 | Timer | sway clock |
| 0x6464 | u8 | "jumped" flag (set by op 4, consumed by the next op 5) |
| 0x6470 | f32 | cull distance squared |
| 0x6474 | u8 | sway mode (op 33) |

## 4. Script opcodes — VERIFIED (switch in `Stage_Calc`)

Arg columns: `f` float, `i` int, `c` D3DCOLOR (u32), `—` unused. "count" = uses in the 18 files.

| op | name | args | effect | count |
|---|---|---|---|---|
| 0 | set_origin | f x, f y, f z | origin (+0x824) = (x,y,z). (Also stores this and the next record's time/args in fields that are never read.) | 18 |
| 1 | fog | c colour, f near, f far | current fog = (colour, near, far) and fog target = same | 52 |
| 2 | fog_time | i dur | fog start = current fog; duration = dur; elapsed = 0. Used before op 1 at the same time, so it interpolates from the old values to the op-1 values | 25 |
| 3 | halt | — | if no section request: stop processing and do **not** advance the script clock this frame (stays here until a section jump); else clear the request and continue | 0 |
| 4 | jump | i index, i time | instruction index = index; clock = time; position interpolation cancelled (duration 0); jumped-flag = 1; continue in the same frame | 35 |
| 5 | cam_pos | f x, f y, f z | if jumped-flag: shift every effect of kind 51 by (new − old target) and clear the flag. start.pos = target.pos; target.pos = (x,y,z); if no position interpolation is active, position = (x,y,z) immediately | 213 |
| 6 | cam_pos_time | i dur, i mode | position interpolation: duration = dur, elapsed = 0, mode | 95 |
| 7 | cam_facing | f x, f y, f z | as op 5 for facing (no jump handling) | 53 |
| 8 | cam_facing_time | i dur, i mode | | 19 |
| 9 | cam_up | f x, f y, f z | as op 5 for up | 26 |
| 10 | cam_up_time | i dur, i mode | | 0 |
| 11 | fov | f fov | start.fov = target.fov; target.fov = fov; if no fov interpolation active, fov = value | 18 |
| 12 | fov_time | i dur, i mode | | 0 |
| 13 | clear_color | c colour | clear colour (+0x830) | 11 |
| 14 / 15 | cam_pos_start / cam_pos_end | f x, y, z | set start.pos / target.pos directly (for Hermite) | 0 |
| 16 / 17 | cam_pos_tan0 / tan1 | f x, y, z | start / end tangent of position | 0 |
| 18 | cam_pos_hermite | i dur | position interpolation with mode 7 (Hermite) | 0 |
| 19–23 | (facing) start, end, tan0, tan1, hermite | | same as 14–18 for facing | 0 |
| 24–28 | (up) start, end, tan0, tan1, hermite | | same as 14–18 for up | 0 |
| 29 | bg_anm_a | i script | start ANM script on VM A (arg < 0: VM A's sprite index = −1, i.e. disabled) | 24 |
| 30 | bg_anm_b | i script | same for VM B. **Quirk**: when the arg is < 0 the code disables VM *A* (writes VM A's sprite index), not VM B | 0 |
| 31 | section | i id | no-op when executed; target of a section request (§4.1) | 17 |
| 32 | cam_eye_offset | f x, f y, f z | eye offset = (x,y,z) | 10 |
| 33 | sway | i mode | sway mode = mode (low byte); sway clock = 0 | 18 |
| 34 | bg_anm_c | i script | same for VM C (< 0 disables VM C) | 7 |

Any other opcode is ignored (index just advances).

### 4.1 Section requests — VERIFIED
At the start of `Stage_Calc`, if `section_request ≠ 0`: scan the script from record 0 for an op 31 whose arg
equals the request. If found: instruction index = (found index + 1), script clock = that record's time,
request = 0. If not found the request stays pending. (ECL opcode 147 = 0x93 writes the request, 07 §7.)
Stage index 7 (6B) additionally drives a special effect VM on requests 1–4 (later phase).

## 5. Per-frame update (`Stage_Calc`) — VERIFIED, in this order

```
if no std loaded or (global flag 0x164D0B4 bit 10): return
(stage 6B special effect handling)
handle section request (§4.1)
loop:
   rec = script[index]
   if rec.time == -1 or clock.current < rec.time: break
   execute rec (§4); index += 1           (op 4 sets index itself; op 3 breaks out, see table)
camera vector interpolation, slots 0 (pos), 1 (facing), 2 (up), each if its duration ≠ 0   (§6)
fov interpolation if active                                                               (§6)
facing_norm = normalize(facing)
sway (§7)
fog interpolation (§8)
if the current record's opcode ≠ 3: clock.Tick()
Stage_UpdateObjects()                                                                      (§9)
(spell-background VMs when stage state +0xB24 > 0 — later phase)
run VM A if its sprite index > 0; run VM B if > 0; run VM C if > 0 and copy VM C's colour to the clear colour
every 3rd frame (frame counter % 3 == 0) if frame counter > 699 or spell practice, and +0xB24 < 2:
    spawn type 0x3E (colour 0x20FFFFFF, layer 4) at **all 12** sparkle points 0..11 (§11.1), whatever the
    recorded count: unused points are (0, 0, 0) from the per-frame clear in Stage_DrawLow, so those sparkles
    appear at screen (0, 0) under the HUD frame (faithful quirk; VERIFIED)
frame counter += 1
```

Note: VM A/B/C are run/drawn only when their **sprite index** (vm+0x214) is > 0 — i.e. after the script has executed an op 3 with a sprite other than 0; disabling sets it to −1.

## 6. Camera interpolation — VERIFIED (0x408D60 for vectors, inline for fov)

For slot s with duration d ≠ 0:
```
if not (elapsed.current < d):  elapsed.Set(d); t = 1; d = 0       # finished
else: elapsed.Tick(); t = elapsed.GetF() / d
apply easing (mode): 1: 1−(1−t)²   2: 1−(1−t)³   3: 1−(1−t)⁴   4: t²   5: t³   6: t⁴   0/other: t
value = start + (target − start)·t                                  (per component)
mode 7 (vectors only): value = Hermite(start, target, tan0, tan1, t) per component with
    H = (2t+1)(t−1)²·start + (3−2t)t²·target + t(1−t)²·tan0 + (t−1)t²·tan1
```
Note the easing numbering here differs from the ANM VM (here 1–3 are ease-out, 4–6 ease-in).
fov uses the same formula with modes 0–6 (no Hermite).

## 7. Sway (op 33) — VERIFIED, constants from .rdata

`c = sway_clock.GetF()`; `φ = (c·π + c·π)/P − π`.

| mode | P | effect | clock wraps when ≥ |
|---|---|---|---|
| 1 | 480 | eye_offset.x = sin φ · 40 | 480 |
| 2 | 480 | eye_offset.x = sin φ · 70; up.x = −sin φ · 0.1 | 480 |
| 3 | 4800 | up.x = sin φ; up.z = cos φ | 4800 |

After computing, `sway_clock.Tick()`; if clock ≥ wrap value it is Set(0). Mode 0 = off.

## 8. Fog — VERIFIED

Interpolation (when duration ≠ 0): `elapsed.Tick(); t = min(1, elapsed.GetF()/d)`; each of the 4 colour bytes
`= trunc((target − start)·t + start)`; near and far linear; when elapsed ≥ d the duration becomes 0.
Fog is D3D **vertex fog, linear** (device init 0x442F60: FOGTABLEMODE = NONE, FOGVERTEXMODE = LINEAR,
FOGDENSITY 1 unused). Colour/start/end are set from the current values every frame by `Stage_DrawLow` (colour
multiplied by the global ANM tint when active; start/end written as floats). RANGEFOGENABLE is never set (all 36
SetRenderState sites checked), so the fog distance is the **eye-space z**, not the radial distance.
Fog is enabled only around `Draw3DWorld` stage quads (§10.3). Their vertices are untransformed (FVF XYZ[|DIFFUSE]|TEX1),
so the pipeline computes the fog from world × view. With the stage projection (PerspectiveFovLH, near 30,
far 1800, §10.1), w_clip = eye z. Billboard sprites and ribbons get software fog instead (§11.1, §11.3; fog is off
for them). cfg+0x38 bit 10 disables fog entirely (FOGENABLE 0 at init, all fog on/off calls skipped).

Fog model to reproduce (VERIFIED at runtime against 6 wine screenshots of the 4 demos, mean |Δ| 0.2–0.4/255 on
the playfield, changelog/answers_impl13.md §4):
1. Clip the triangle to the near/far planes (0 ≤ z ≤ w).
2. Interpolate the eye depth d perspective-correctly (= the pixel's true eye z).
3. Per pixel: f = clamp((end − d)/(end − start), 0, 1); rgb = f·rgb + (1 − f)·fog rgb; alpha unchanged; then
   alpha test/blend as usual.
Clamping f per vertex and interpolating it (the textbook D3D8 vertex-fog description) is visibly wrong against
the reference. Walls that span `start` come out 10–12 levels too dark.

## 9. Object update (`Stage_UpdateObjects`) — VERIFIED

1. Overlay VM (+0x844): when the player mode byte 0x17D5EF8+5 (player+5: 0 human, 1 youkai; §13) becomes non-zero, send interrupt 1;
   when it becomes zero again, interrupt 2. Run it.
2. For every object with flag bit 0: run the VM of every quad (types 0 and 1); count VMs whose script has not
   finished. If the **last** quad's VM has tag (vm+0x1FC) == 1: set its "use colour2" flag and set
   colour2 = colour × overlay-VM colour / 256 per byte (R, G, B, A). If no VM is still running, clear the
   object's bit 0 (it will no longer be updated; drawing is not affected by this bit).

## 10. Drawing

### 10.1 Viewport and projection — VERIFIED (answers implementer Q1)

`Stage_DrawLow` sets the global viewport record to **x = 32, y = 16, w = 384, h = 448** and calls
`SetViewport` with it (MinZ/MaxZ = 0/1 from the zero-initialised record — INFERRED) — a real D3D viewport, so
the 3D projection is mapped into the playfield rectangle (not just clipped). `Stage_DrawLayer` then sets the
stage camera (`Stage_SetCamera` 0x40B6D0):

- View = LookAtLH(eye = position + eye_offset, at = position + facing, up = up).
- Projection = PerspectiveFovLH(fovY = fov (+0x63DC), aspect = 384/448 (viewport w/h), near = **30**,
  far = **1800**).
- (It also stores cross(…, up) normalised at +0x63C4 — unused by the stage draw.)

After layers 2–3 `Stage_DrawHigh` switches back to the default 2D camera (`Camera_Set2D`, 05 §9) for the same
viewport, and sets fog start/end to 1000/2000.

### 10.2 Frame order — VERIFIED

`Stage_DrawLow` (prio 6):
1. Reset the sparkle-point list (16 × vec3 at +0x6480 = 0, count = 0). Viewport = playfield (above).
   Flush/reset ANM render-state caches.
2. Optionally (+0xB2C) clear the playfield to black + z once.
3. Global ANM tint (AnmManager+4/+0) is turned on (`AnmMgr_TintOn` 0x40BAD0, colour +0x6468) if a request
   exists (alpha byte +0x646B ≠ 0); the request is then reset to 0x00808080 (alpha 0). Requests come from
   `Stage_RequestTint` 0x409160 (the bomb tint 0x40BC60, 14 §1.3): an empty request takes the colour, a second
   request in the same frame averages each channel with the stored one. So the tint drawn in frame N is the
   request made by the bomb draw of frame N − 1 (VERIFIED, changelog/answers_impl11.md §1).
4. Unless stage state +0xB24 ≥ 2 or a HUD condition: draw VM A and VM B (draw mode "2D rotated" 05 §9, if
   sprite index > 0), stage 6B special effect.
5. Clear: if clear colour alpha == 0xFF → Clear(target + z, colour, z = 1); if clear colour == 0 → Clear(z only);
   otherwise fill the playfield rectangle (32,16)–(416,464) with the colour (alpha-blended) and Clear(z only).
6. ZFUNC = LESSEQUAL; fog colour/near/far; fog on.
7. Unless +0xB24 ≥ 2 or the HUD condition: `Stage_DrawLayer(0)`, `Stage_DrawLayer(1)`.

`Stage_DrawHigh` (prio 7): layers 2 and 3 (same condition); fog off; effects draw; ZFUNC = ALWAYS; extra
spell-background VMs when +0xB24 > 0; disable "stage camera" render path; default 2D camera; viewport; at the
end: if the "bomb drew last frame" flag +0x646C (0x4EA49C) is 0 → global tint off (`AnmMgr_TintOff` 0x40BAB0:
AnmManager+4 := 0, colour 0x80808080); then +0x646C := 0 and +0x647C := 0.
Tint lifetime while a bomb runs (VERIFIED, changelog/answers_impl11.md §1): `Bomb_DrawTint` sets +0x646C := 1 at player
draw prio 9 every bomb frame, so at the next frame's `Stage_DrawHigh` the tint is **not** switched off. It stays
on for everything drawn until something turns it off: stage layers 0–3, background effects (list B) and the
spell-background VMs, the prio-8 enemy pass (layers 0–1) and the player's flying bullets (prio 9, drawn before
the bomb draw fn) are tinted; `Bomb_DrawTint` itself turns it off first thing. The prio-11 enemy pass,
`BulletMgr_Draw` (13) and `Spell_Draw` (15) set/clear the tint themselves (on with 0xFFF01010 only while GameFlags
bit 10, then off). `Player_UpdateBomb` (bomb end) and `Player_DyingUpdate` also turn it off (calc). Edge frames:
first bomb frame → no request yet, no stage tint; frame after the last bomb draw → the pending request still
tints that frame's stage and +0x646C (set by the last bomb draw) keeps it on through prio 8/9 (the calc-time
tint-off of the bomb end happens before the draw, so it does not prevent this); the frame after that is clean.

### 10.3 `Stage_DrawLayer(layer)` — VERIFIED

```
set stage camera; enable stage-camera mode in the ANM manager
right = normalize(view matrix row 0 (_11,_12,_13))          # camera right axis in world space
for each instance in list order:
  obj = objects[instance.obj]; if obj.layer != layer: continue
  centre = obj.pos + instance.pos − origin + obj.size/2
  rel = centre − (cam.position + cam.eye_offset)
  if |rel|² > cull_dist²: continue
  d = dot(rel, facing_norm)
  if not (80 ≤ d ≤ |obj.size|/2 + 960): continue
  obj.flags |= 2
  for each quad of obj:
    vm = VMs[quad.vm_index]
    type 0: vm.pos = vm.pos_offset + quad.pos + instance.pos − origin
            if quad.w ≠ 0: vm.scale.x = quad.w / sprite.width
            if quad.h ≠ 0: vm.scale.y = quad.h / sprite.height
            if (vm.tag & 0xF) == 2: billboard sprite (§11.1)
            else: 3D world quad (§11.2), fog on
    type 1: ribbon (§11.3)
```

### 11.1 Billboard sprite (tag & 0xF == 2) — VERIFIED
- `s = project(vm.pos)` (D3DXVec3Project with the stage viewport/projection/view, world = identity).
- `W` = quad.w if ≠ 0 else sprite width; `s2 = project(vm.pos + right·W·scale.x)`;
  `scale.x = scale.y = |s2 − s| / W`; if W < 0, scale.y = −scale.y.
- Distance `dist = |vm.pos − (cam.position + eye_offset)|`. If dist > fog_near:
  `f = (near − dist)/(near − far)`; if f ≥ 1 the quad is skipped; else per colour channel c (B, G, R):
  `c = c − trunc((c − fog_c)·f)` (byte arithmetic) and `alpha = trunc(alpha·(1 − f))` (temporary: the VM
  colour is restored after drawing).
- vm.pos = s (screen x, y, depth z). If 0 ≤ z ≤ 1: fog off, draw with ANM draw mode 1 (2D, no snapping,
  05 §9), using the projected z as vertex depth. If additionally (tag & 0xF0) == 0x10 and dist < fog_near and
  the sparkle flag (+0x647C) is set, append (s.x, s.y, 0) to the sparkle-point list (no bounds check; the 32-entry
  array is never exceeded by the shipped data — at most 15 points per frame, changelog/answers_impl10.md §3, VERIFIED).
- Note: vm.pos is left at the projected screen position; it is recomputed every draw.

### 11.2 3D world quad (`AnmManager_Draw3DWorld` 0x464470) — VERIFIED
Requires visible, enabled, alpha ≠ 0. World matrix as ANM mode 5 (base matrix scaled by scale.x/y, then
× RotX × RotY × RotZ, rebuilt only when dirty and not frozen), translation = vm.pos (+half size if anchor
bits) **plus the global screen-shake offset on x and y**. Render state from the VM (blend, colour as texture
factor, z-write, …). Texture matrix = vm.tex_matrix with its third row's first two entries set to
(sprite.u0 + uv_off_u, sprite.v0 + uv_off_v), uploaded only when the sprite differs from the last one drawn
**or uv_off_u ≠ 0** (the code tests uv_off_u twice and never uv_off_v — quirk: a pure-v scroll with an
unchanged sprite keeps the previous texture matrix). Geometry: the unit quad
(−128,−128,0)…(128,128,0) with UV (0,0)…(1,1) as a triangle strip.

### 11.3 Ribbon (type 1) — VERIFIED
Points A = quad.A + instance.pos − origin, B likewise. For each point P: `sP = project(P)`,
`wP = |project(P + right·W) − sP| / 2` (W = quad width or sprite width), and fog colour as in §11.1
(f ≥ 1 → alpha 0). With `dir = normalize(sB.xy − sA.xy)` (skipped if length < 1e-5) and both depths in
[0,1], four pre-transformed vertices (rhw 1):
v0 = (sA.x + dir.y·wA, sA.y − dir.x·wA), v1 = (sA.x − dir.y·wA, sA.y + dir.x·wA),
v2 = (sB.x + dir.y·wB, sB.y − dir.x·wB), v3 = (sB.x − dir.y·wB, sB.y + dir.x·wB);
UV: v0 (u0,v0), v1 (u1,v0), v2 (u0,v1), v3 (u1,v1) (sprite rect + uv offset); colours: v0,v1 from A's fog,
v2,v3 from B's fog. Drawn through the 2D batch (fog off).
Details (VERIFIED, changelog/answers_impl17.md §5.1, demo 1 draw log):
- `wP` uses the **3D** length (`Vec3_Length` of the projected x, y, depth difference), like §11.1.
- Point colour by distance d = |P − (cam.position + eye_offset)|: d ≤ fog_near → the VM colour (+0x1F0, colour 1, not
  the stage-tint colour 2); near < d with f < 1 → fogged as §11.1; **f ≥ 1 → only the alpha byte is cleared, the RGB
  bytes keep whatever the vertex slot held** (the four vertex colours live in one stack array that is reused for every
  ribbon of the `Stage_DrawLayer` call). So a fully fogged end keeps the RGB of the previous ribbon's same-end colour
  (A ends share local slots v0/v1, B ends v2/v3). Before any ribbon of the call has written them, the original shows
  A = (0x910D20 for v0, 0x676E85 for v1) and B = 0 (stack residue, identical in every frame of demo 1 — INFERRED
  constant). This matters slightly: the RGB of an alpha-0 end is interpolated toward the visible end (alpha test ≥ 4).
- The colours are computed (and the slots updated) before the direction/depth checks, so a skipped ribbon still
  changes the stale values.
- Stage 4A (`stg4abg.anm` script 1, sprite 1 = 16 × 512 at (496, 0), tag 2) is the only stage using ribbons in the
  demos; in demo 1 ≈ 68 ribbons per frame, A end usually fully fogged (alpha 0), B end alpha 0…0x60.

## 12. Open points
- The 5 text fields at 0x10 are unused; the instance field +2 (always 256) is unused.
- Stage state +0xB24/+0xB28 (stage transition / spell background) and the sparkle effects: later phases.

## 13. Implementer questions answered in Phase 3 (VERIFIED)
- **Overlay VM (+0x844) ANM file**: global 0x17CE8F4 = `text.anm` loaded into ANM slot 0 at startup (0x4464C8).
  Script 33 of text.anm: sprite 21; on interrupt 2 colour → (R 64, G 64, B 255) over 16 frames with alpha 255; on
  interrupt 1 colour → white over 16 frames. `Stage_LoadStd` starts it and sends interrupt 2; each frame
  `Stage_UpdateObjects` switches it by the **player mode byte** (player+5): when the mode becomes youkai (≠ 0)
  → interrupt 1 (white), when it becomes human (0) → interrupt 2 (blue); latch +0x834 (1 = youkai), timer
  +0x838 reset on each switch. The VM colour is the stage tint of §5 (colour2 = colour × overlay / 256). So the
  stage objects are tinted blue in human mode and drawn normally in youkai mode.
- **Byte 0x17D5EF8+5**: owned by the player — the active character / "mode" (0 human, 1 youkai): a copy of the
  focus state once it has been held for more than 6 frames; solos: shot-type parity (09 §1.1, §3).
- **Ribbons (§11.3)**: vertex z = projected depth of A for v0/v1 and of B for v2/v3 (rhw 1). Drawn only if the
  VM is enabled+visible with colour alpha ≠ 0 (`DrawCustomQuad`), the projected direction length ≥ 1e-5 and both
  projected depths are in [0, 1] (plus the object-level culling of §11). The fog result only changes the vertex
  colours/alpha (a fully fogged point gives alpha 0 vertices; the quad is still submitted).
- **Billboard scale (§11.1)**: `|s2 − s|` is the **3D length** of the difference of the two projected points
  (screen x, y and projected depth z, `Vec3_Length`). s = project(origin) with world = translation(vm.pos);
  s2 uses translation(vm.pos + right·W·**vm.scale.x**) — the VM's current scale.x, which (when quad.w == 0) is the
  value written by the previous frame's billboard computation (feedback, faithful).
- **Seams between ground tiles**: textures are sampled with ADDRESSU/V = WRAP and LINEAR min/mag filtering, no
  mip-mapping (05 §12.4); 3D world quads use UV 0..1 through the texture matrix with **no half-texel offset**
  (only 2D draw modes 0/2 snap with −0.5). Linear filtering at the tile edge therefore blends with the wrapped
  opposite edge of the sprite rect/texture exactly as the original does; a reimplementation should use the same
  sampler state (wrap + bilinear) and the same UV mapping rather than clamping.
