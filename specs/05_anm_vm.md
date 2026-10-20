# 05 — ANM script VM (sprite animation interpreter)

Source: th08.exe 1.00d. The interpreter is `AnmVm_Run` (0x45EA00); helpers and draw functions are named in
`symbols.csv`. File/container layout of `.anm` is in 03_images.md. Tool: `tools/anmdis.py` disassembles every
script of every `.anm` (full listing: `extracted/anm_dis.txt`). Over all 113 files (1151 scripts) every opcode
in the data is known, every instruction size matches its signature, every jump target lands on an instruction
boundary, and scripts are contiguous (the next script starts right after the previous one's `end`).

**VERIFIED** = read from code (and data consistent). **INFERRED** = interpretation / naming.

## 1. Instruction encoding — VERIFIED

```
+0  i16  opcode            (-1 = end of script)
+2  u16  size in bytes     (header + args; next instr = this + size)
+4  i16  time              (absolute script time at which the instr executes)
+6  u16  var mask          (bit k set => argument k is a variable id, see §3)
+8  args                   4 bytes each (int32 or float32 depending on opcode)
```

Scripts are referenced by the file's script table (03_images.md); the script id field is ignored, the
index into the table is what game code uses. Jump/branch offsets are **byte offsets from the script start**.

## 2. Per-instance state: `AnmVm` (0x2A4 bytes)

Offsets are VERIFIED by the code that reads/writes them; the "meaning" column is VERIFIED where it is the
direct effect of an opcode/draw routine, INFERRED for names of owner-only fields.

| off | type | name | meaning |
|---|---|---|---|
| 0x000 | f32[3] | rotation | x, y, z radians (2D draw uses z only) |
| 0x00C | f32[3] | angular_vel | added to rotation every frame (× game speed) |
| 0x018 | f32[2] | scale | x, y (1.0 = sprite size) |
| 0x020 | f32[2] | scale_growth | added to scale every frame (× game speed) |
| 0x028 | f32 | sprite_w | width of current sprite in logical px (from sprite record) |
| 0x02C | f32 | sprite_h | height |
| 0x030 | f32 | uv_off_u | texture-coordinate offset added to the sprite's normalised u (wraps in [0,1)) |
| 0x034 | f32 | uv_off_v | same for v |
| 0x038 | Timer | time | script clock |
| 0x044 | Timer | wait_timer | used by op 79 |
| 0x050 | Timer[7] | interp_elapsed | slot order: 0 pos, 1 colour, 2 alpha, 3 rotation, 4 scale, 5 colour2, 6 alpha2 |
| 0x0A4 | Timer[7] | interp_duration | same slot order; duration ≤ 0 = slot inactive |
| 0x0F8 | u8[7] | interp_mode | easing mode per slot (§6) |
| 0x100 | i32[4] | I0..I3 | int variables 10000..10003 |
| 0x110 | f32[4] | F0..F3 | float variables 10004..10007 |
| 0x120 | i32[2] | I4, I5 | int variables 10008, 10009 |
| 0x128 | f32 | uv_scroll_u | added to uv_off_u every frame (NOT scaled by game speed) |
| 0x12C | f32 | uv_scroll_v | |
| 0x130 | Matrix4 | base_matrix | identity with [0][0] = w/256, [1][1] = h/256 (set with the sprite) |
| 0x170 | Matrix4 | world_matrix | cached model matrix for draw mode 5 |
| 0x1B0 | Matrix4 | tex_matrix | identity with [0][0] = (w / tex_w_actual)·sprite.scale_x, [1][1] = (h / tex_h_actual)·sprite.scale_y (INFERRED: UV matrix for 3D stage quads) |
| 0x1F0 | u32 | colour | D3DCOLOR: byte 0x1F0 = B, 0x1F1 = G, 0x1F2 = R, 0x1F3 = A |
| 0x1F4 | u32 | colour2 | second colour, same byte order (0x1F7 = alpha2) |
| 0x1F8 | u32 | flags | §2.1 |
| 0x1FC | u16 | tag | free value written by op 25 (`set_tag`); read only by owner code. Stage/enemy use it as a value. **Player bullets: tag ≠ 0 → each frame before drawing, rotation z := bullet angle** (14 §5, 09 §4.5; changelog/answers_bugs1.md B1) |
| 0x1FE | u16 | pending_interrupt | 0 = none; set by owners (`AnmVm_Interrupt` 0x407120) |
| 0x200 | i32 | draw_mode | 0..5, used by the generic draw entry (§8) |
| 0x204 | ptr | anm_file | loaded-file record (structs.md `AnmLoadedFile`) |
| 0x208 | f32[3] | pos | position x, y, z |
| 0x214 | i16 | sprite_index | index into the file's sprite table |
| 0x216 | i16 | anm_slot | ANM slot of the file |
| 0x218 | i16 | saved_sprite | owner copy of sprite_index (INFERRED) |
| 0x21A | i16 | script_index | < 0 = no script (range runner skips it) |
| 0x21C | ptr | script_start | |
| 0x220 | ptr | ip | current instruction; NULL = script finished |
| 0x224 | ptr | sprite | runtime sprite record (0x44 bytes, structs.md) |
| 0x228 | Timer | saved_time | time saved on interrupt entry |
| 0x234 | ptr | saved_ip | ip saved on interrupt entry |
| 0x238 | f32[3] | pos_start | interpolation start / end values: |
| 0x244 | f32[3] | pos_end | |
| 0x250 | f32[3] | rot_start | |
| 0x25C | f32[3] | rot_end | |
| 0x268 | f32[2] | scale_start | |
| 0x270 | f32[2] | scale_end | |
| 0x278 | u8[4] | colour_start | B, G, R at 0x278..0x27A; alpha_start at 0x27B |
| 0x27C | u8[4] | colour_end | B, G, R at 0x27C..0x27E; alpha_end at 0x27F |
| 0x280 | u8[4] | colour2_start | B, G, R, alpha2_start (0x283) |
| 0x284 | u8[4] | colour2_end | B, G, R, alpha2_end (0x287) |
| 0x288 | f32[3] | pos_offset | written instead of `pos` when flag bit 8 is set; added to pos by owner code (VERIFIED for `Result_Draw` 0x4586B4, 16 §2; other owners per their sections) |
| 0x294 | i32 | sprite_set_time | script time when op 3 last ran |
| 0x298, 0x299 | u8 | — | set to 15 by `AnmManager_SetScript`; owner-only (unknown) |

`Timer` (12 bytes) — VERIFIED (0x406610, 0x406660, 0x447421):

| off | type | meaning |
|---|---|---|
| 0 | i32 | previous value (−999 after a Set) |
| 4 | f32 | sub-frame fraction |
| 8 | i32 | current value |

- `Set(v)`: current = v, fraction = 0, previous = −999.
- `Tick()`: previous = current; if game_speed > 0.99 → current += 1; else fraction += game_speed and if
  fraction ≥ 1 → current += 1, fraction −= 1. (Game speed is 1.0 in normal play; slow-down effects set 1/N.)
- `Decrement()` (`Timer_Sub(1)`): if game_speed > 0.99 → current −= 1; else fraction −= speed, borrowing
  from current while fraction < 0. (There is an extra unconditional decrement when Supervisor flag 0x20 is set;
  that flag is set only by the slow-down effect restore — a later-phase detail.)
- `GetF()` = current + fraction.

### 2.1 flags (vm+0x1F8) — VERIFIED bit usage

| bit | meaning |
|---|---|
| 0 | visible (draw functions skip the sprite when clear) |
| 1 | enabled (also required for drawing; never changed by scripts, set at init) |
| 2 | rotation changed (mode-5 matrix dirty) |
| 3 | scale changed (mode-5 matrix dirty) |
| 4–5 | blend mode: 0 = alpha (dest = INVSRCALPHA), 1 = additive (dest = ONE); 2/3: the dest-blend state is left unchanged (whatever the previous sprite used) |
| 8 | op 6 / position interpolation write `pos_offset` (0x288) instead of `pos` |
| 9 | flipped X (toggled by op 10) |
| 10 | flipped Y (toggled by op 11) |
| 11 | anchor: x is the left edge instead of the centre |
| 12 | anchor: y is the top edge instead of the centre |
| 13 | z-write disabled (D3DRS_ZWRITEENABLE = 0 when set, unless a config flag forces otherwise) |
| 14 | waiting for an interrupt (set by op 20/23) |
| 15 | use the stage's 3D camera instead of the default 2D camera (view/projection switched before drawing) |
| 16 | freeze mode-5 world matrix (do not rebuild) |
| 17 | draw with colour2 instead of colour |
| 19 | paused: `AnmVm_Run` does nothing and returns 0 |

Initial flags after init = 0x0007 (visible, enabled, rotation-dirty); `AnmFile_StartScript` then clears
visible and the flip bits before the first run.

## 3. Arguments and variables — VERIFIED (0x45E650, 0x45E720, 0x45E810, 0x45E8A0)

For each argument k whose mask bit k is set, the 4-byte arg is a **variable id** instead of a literal:

| id | int read | float read | int dest | float dest |
|---|---|---|---|---|
| 10000–10003 | I0–I3 | (float)I0–I3 | I0–I3 | — |
| 10004–10007 | trunc(F0–F3) | F0–F3 | — | F0–F3 |
| 10008, 10009 | I4, I5 | (float)I4, I5 | I4, I5 | — |
| anything else | literal value | literal value | **the arg slot inside the script itself** | **the arg slot itself** |

- Float-typed args holding a variable id store it as a float (e.g. 10004.0); it is converted to int by
  truncation before the lookup. An unrecognised id returns the literal (float) value.
- Destination args (`V`/`W` below) only redirect if mask bit 0 is set and the id is a valid variable of the
  right kind; otherwise the instruction **writes into its own argument bytes in the loaded script**. Scripts
  are shared by all instances using the same file, so such writes persist (relevant for op 5 with a literal
  counter; in th08 data every op 5 uses `$I4`).
- All variables are zeroed by `AnmVm_Init` (they lie in the first 0x208 bytes that init clears).
- int ↔ float conversions: float→int = truncation toward zero (`crt_ftol`).

## 4. Per-frame update: `AnmVm_Run(vm)` — VERIFIED

Called by each owner once per frame (calc phase). Returns 1 when the script has finished (ip == NULL),
otherwise 0.

```
if vm.ip == NULL: return 1
if flags.paused(bit19): return 0
if vm.pending_interrupt != 0: goto DISPATCH_INTERRUPT
LOOP:
  ins = vm.ip
  if vm.time.current < ins.time: goto POST
  execute ins (table §5)
  default: vm.ip = ins + ins.size; goto LOOP
POST:
  rotation update; interpolations; scale growth; uv scroll   (§6)
  vm.time.Tick()
  anm_manager.vm_counter += 1      (statistics only)
  return 0

DISPATCH_INTERRUPT:
  scan from script_start, instruction by instruction, until an op 21 whose arg == pending_interrupt
  or an op -1; while scanning remember the most recent op 21 with arg == -1 ("catch-all")
  pending_interrupt = 0; flags.bit14 = 0
  if no matching label: if a catch-all was seen use it; else { time.Decrement(); goto POST }
  saved_time = time; saved_ip = ip                       (op 89 returns here)
  ip = label + label.size                                (the instruction after the label)
  time.Set(ip.time)                                      (time of that instruction)
  flags.visible = 1
  goto LOOP
```

Consequences (VERIFIED): all instructions whose time ≤ current time run in the same frame, in order. Jumps
set the clock to their target time and continue immediately in the same frame. Time 0 instructions run
during `AnmFile_StartScript` (the script is run once when started), so a freshly-started VM has already
executed its frame-0 instructions and its clock is 1 when the owner first runs it.

"Yield" opcodes (20, 23, 79 when waiting) decrement the clock and go to POST; POST ticks it again, so the
clock stays the same and the same instruction is re-executed next frame.

## 5. Opcode table — all VERIFIED from `AnmVm_Run`

Arg letters: `i` int, `f` float, `V` int var dest, `W` float var dest, `O` byte offset from script start,
`T` time to set on jump. "count" = uses in th08 data (0 = supported but unused).

| op | name | args | effect | count |
|---|---|---|---|---|
| −1 | end | — | visible = 0; ip = NULL; return 1 (script finished) | 1151 |
| 0 | nop | — | nothing | 51 |
| 1 | hide_stop | — | same as −1 | 528 |
| 2 | stop | — | ip = NULL; return 1 (keeps visible) | 219 |
| 3 | sprite | i idx | visible = 1; set sprite idx of this file (§7); sprite_set_time = time | 2826 |
| 4 | jump | O off, T t | time.Set(t); ip = start + off; continue | 319 |
| 5 | loop_dec | V var, O off, T t | var −= 1; if var > 0: time.Set(t), ip = start + off | 20 |
| 6 | pos | f x, f y, f z | pos (or pos_offset if bit 8) = (x,y,z) | 898 |
| 7 | scale | f sx, f sy | scale = (sx, sy); bit 3 | 208 |
| 8 | alpha | i a | alpha byte = a (low 8 bits) | 817 |
| 9 | color | i r, i g, i b | colour R,G,B bytes = r,g,b | 87 |
| 10 | flip_x | — | toggle bit 9; scale.x *= −1; bit 3 | 78 |
| 11 | flip_y | — | toggle bit 10; scale.y *= −1; bit 3 | 0 |
| 12 | rotation | f x, f y, f z | rotation = (x,y,z); bit 2 | 53 |
| 13 | angular_vel | f x, f y, f z | angular_vel = (x,y,z); bit 2 | 119 |
| 14 | scale_growth | f x, f y | scale_growth = (x,y) | 24 |
| 15 | alpha_fade | i a, i dur | alpha slot: start = current alpha, end = a, mode 0, elapsed = 0, duration = dur | 351 |
| 16 | blend_additive | i b | blend bits = (b ≠ 0) ? 1 : 0 | 61 |
| 17 | pos_time_linear | f x, f y, f z, i dur | pos slot, mode 0: start = current pos (or pos_offset if bit 8), end = (x,y,z) | 181 |
| 18 | pos_time_out2 | f x, f y, f z, i dur | same, mode 4 | 359 |
| 19 | pos_time_out4 | f x, f y, f z, i dur | same, mode 6 | 0 |
| 20 | wait_interrupt | — | if pending_interrupt == 0: bit 14 = 1, time.Decrement(), go to POST (yield). Else dispatch interrupt | 995 |
| 21 | interrupt_label | i id | no-op when executed; target of interrupt id (−1 = catch-all) | 2173 |
| 22 | anchor_top_left | — | bits 11 and 12 = 1 | 408 |
| 23 | hide_wait_interrupt | — | visible = 0, then as op 20 | 749 |
| 24 | pos_target_offset | i b | bit 8 = b & 1 | 408 |
| 25 | set_tag | i v | tag (u16) = v | 91 |
| 26 | uv_add_u | f d | uv_off_u += d, wrapped (§6.4) | 4 |
| 27 | uv_add_v | f d | uv_off_v += d, wrapped | 28 |
| 28 | visible | i b | bit 0 = b & 1 | 2 |
| 29 | scale_time_linear | f sx, f sy, i dur | scale slot, mode 0: start = current scale, end = (sx,sy) | 0 |
| 30 | zwrite_off | i b | bit 13 = b & 1 | 239 |
| 31 | stage_camera | i b | bit 15 = b & 1 | 48 |
| 32 | pos_time | i dur, i mode, f x, f y, f z | pos slot with easing `mode` (low byte) | 899 |
| 33 | color_time | i dur, i mode, i r, i g, i b | colour slot: start = current R,G,B; end = r,g,b | 711 |
| 34 | alpha_time | i dur, i mode, i a | alpha slot: start = current alpha; end = a | 1124 |
| 35 | rotation_time | i dur, i mode, f x, f y, f z | rotation slot: start = current rotation; end = (x,y,z); bit 2 | 2 |
| 36 | scale_time | i dur, i mode, f sx, f sy | scale slot: start = current scale; end; bit 3 | 239 |
| 37 | iset | V a, i b | a = b | 120 |
| 38 | fset | W a, f b | a = b | 8 |
| 39/40 | iadd/fadd | V/W a, b | a += b | 0/1 |
| 41/42 | isub/fsub | a, b | a −= b | 0/4 |
| 43/44 | imul/fmul | a, b | a *= b | 0/2 |
| 45/46 | idiv/fdiv | a, b | a /= b (int: C truncating division) | 0/0 |
| 47/48 | imod/fmod | a, b | a = a % b (int: C remainder; float: x87 `fmod(a, b)`) | 0/0 |
| 49/50 | iadd3/fadd3 | a, b, c | a = b + c | 11/11 |
| 51/52 | isub3/fsub3 | a, b, c | a = b − c | 0/13 |
| 53/54 | imul3/fmul3 | a, b, c | a = b · c | 0/0 |
| 55/56 | idiv3/fdiv3 | a, b, c | a = b / c | 2/4 |
| 57/58 | imod3/fmod3 | a, b, c | a = b % c / fmod(b, c) | 0/0 |
| 59 | irand | V a, i n | a = n == 0 ? 0 : (Rng_Next32() mod n) (unsigned) | 11 |
| 60 | frand | W a, f x | a = Rng_NextFloat() · x, with Rng_NextFloat = u32 / 2^32 ∈ [0,1) | 22 |
| 61–65 | fsin / fcos / ftan / facos / fatan | W a, f x | a = sin/cos/tan/acos/atan(x) | 0 |
| 66 | norm_angle | W a | a = wrap(a) into [−π, π] (§6.3) | 0 |
| 67/68 | ije/fje | a, b, O off, T t | if a == b: time.Set(t); ip = start + off | 0 |
| 69/70 | ijne/fjne | | if a ≠ b: jump | 1/0 |
| 71/72 | ijl/fjl | | if a < b | 0 |
| 73/74 | ijle/fjle | | if a ≤ b | 0 |
| 75/76 | ijg/fjg | | if a > b | 0 |
| 77/78 | ijge/fjge | | if a ≥ b | 0 |
| 79 | wait | i n | if wait_timer == 0: wait_timer.Set(n) else wait_timer.Decrement(); if wait_timer > 0: time.Decrement() and yield (POST); else wait_timer.Set(0) and continue | 20 |
| 80 | uv_scroll_u | f s | uv_scroll_u = s | 19 |
| 81 | uv_scroll_v | f s | uv_scroll_v = s | 18 |
| 82 | blend_mode | i m | blend bits = m & 3 | 106 |
| 83 | draw_mode | i m | draw_mode = m (raw, no variable support) | 56 |
| 84 | color2 | i r, i g, i b | colour2 R,G,B | 0 |
| 85 | alpha2 | i a | alpha2 = a | 3 |
| 86 | color2_time | i dur, i mode, i r, i g, i b | colour2 slot (as op 33) | 6 |
| 87 | alpha2_time | i dur, i mode, i a | alpha2 slot | 9 |
| 88 | use_color2 | i b | bit 17 = **bit 8** of the int arg (the code reads the arg's second byte) | 0 |
| 89 | return_interrupt | — | time = saved_time; ip = saved_ip; continue (re-executes the instruction that was current when the interrupt arrived) | 200 |

Notes:
- Ops 59/60 use the global game RNG (`g_Rng` 0x164D520) — script randomness consumes the same stream as
  gameplay (RNG algorithm: later phase).
- Op 79 in data is always used as "wait n frames": with n literal, the instruction stalls the script for n
  frames (the clock does not advance during the wait).
- Ops with `mode` take only the low byte (signed char) of the int.
- Order of operand reads for ops 6/7/12/13/17–19 does not matter (no side effects).

## 6. Post-processing each frame (POST), in this exact order — VERIFIED

`speed` = global game speed (1.0 normally).

1. **Angular velocity**, for each axis x, y, z with angular_vel ≠ 0:
   `rotation = wrap(rotation + speed·angular_vel)`; set bit 2.
2. **Interpolation slots** i = 0..6 (pos, colour, alpha, rotation, scale, colour2, alpha2), for each slot with
   `duration.current > 0`:
   ```
   elapsed.Tick()
   if elapsed.current >= duration.current: t = 1.0; duration.Set(0)        # slot ends this frame
   else: t = elapsed.GetF() / duration.GetF()                              # float
   t = ease(mode[i], t)
   ```
   Easing modes (`ease`): 0 linear `t`; 1 `t²`; 2 `t³`; 3 `t⁴`; 4 `1−(1−t)²`; 5 `1−(1−t)³`;
   6 `1−(1−t)⁴`; any other value: linear.
   Application:
   - pos: `p = (end − start)·t + start` per component, into pos (or pos_offset if bit 8).
   - colour / alpha / colour2 / alpha2: per byte `b = trunc((end − start)·t + start)` computed in floating point
     from the integer bytes, stored as a byte.
   - rotation: per axis `rotation = wrap(start + (end − start)·t)`; set bit 2.
   - scale: `s = (end − start)·t + start` for x and y; set bit 3.
   Because the instruction that starts a slot runs before POST in the same frame, the first update happens in
   that same frame with elapsed = 1 (t = 1/dur); the end value is reached exactly `dur` frames after start.
3. **Scale growth**: if growth.y ≠ 0: `scale.y += speed·growth.y` (bit 3); then if growth.x ≠ 0:
   `scale.x += speed·growth.x` (bits 3 and 2).
4. **UV scroll**: `uv_off_u += uv_scroll_u`, then wrap; same for v (not scaled by speed).
   Wrap (also used by ops 26/27): if u ≥ 1: u −= 1, else if u < 0: u += 1 (single step).
5. `time.Tick()`.

### 6.3 Angle wrap (`Angle_AddNormalize` 0x43EDB0) — VERIFIED
`a = a + d`; then up to 18 times: while a > π: a −= 2π; then up to 18 times: while a < −π: a += 2π
(constants are the float32 values of π = 3.1415927 and 2π = 6.2831855).

## 7. Sprite assignment (`AnmVm_SetSprite` 0x45E430) — VERIFIED

Fails (no change) if the file is not loaded. Otherwise: anm_file, sprite_index, sprite pointer =
`file.sprites[idx]` (the file-wide sprite table, 03_images.md), sprite_w/h = sprite logical width/height;
base_matrix = identity with [0][0] = w/256, [1][1] = h/256; tex_matrix = identity with
[0][0] = (w / sprite.actual_tex_w)·sprite.scale_x, [1][1] = (h / sprite.actual_tex_h)·sprite.scale_y;
world_matrix = base_matrix.

## 8. Starting a script and owner API — VERIFIED

- `AnmManager_SetScript(vm, script_index)` (0x40B910): script_index field = idx; pos = (0,0,0);
  pos_offset = (0,0,0); bytes 0x298/0x299 = 15; then `AnmFile_StartScript(file, vm, file.scripts[idx])`.
- `AnmFile_StartScript` (0x45E580): if the script pointer is NULL or the file is not loaded, the whole VM
  (0x2A4 bytes) is zeroed (ip = NULL → inert). Otherwise `AnmVm_Init` (zero 0x208 bytes; scale = (1,1);
  colour = 0xFFFFFFFF; base_matrix = identity; flags low 16 bits = 7; time reset), then anm_slot,
  anm_file, flip bits cleared, script_start = ip = script, time = 0, visible = 0, and **AnmVm_Run once**.
- Note: SetScript zeroes pos *before* StartScript and `AnmVm_Init` does not touch pos (0x208 lies outside the
  cleared range), so the frame-0 run sees pos = (0,0,0) unless the script sets it; anything the owner writes
  to pos afterwards overrides a time-0 `pos` instruction (VERIFIED from layout/order).
- Interrupt: `vm.pending_interrupt = id` (handled at the next `AnmVm_Run`).
- Range helpers: start consecutive scripts on an array of VMs (0x462360), run an array (0x462310, skips
  VMs with script_index < 0), interrupt an array (0x462270, only VMs whose file is loaded).

## 9. Drawing a sprite instance — VERIFIED

Owners call one of the draw functions directly (most use mode 0 or mode 2 functions);
`AnmManager_DrawByMode` (0x45E960) dispatches on `draw_mode`. All 2D modes build a screen-space quad of
pre-transformed vertices (x, y, z, rhw = 1, diffuse colour, u, v) and require: visible (bit 0), enabled
(bit 1) and alpha ≠ 0; otherwise nothing is drawn.

Common values: `hw = sprite_w·scale.x / 2`, `hh = sprite_h·scale.y / 2` (negative scale → mirrored quad).
Vertex order: v0 top-left, v1 top-right, v2 bottom-left, v3 bottom-right; drawn as two triangles
(v0 v1 v2) and (v1 v2 v3).

| mode | function | geometry |
|---|---|---|
| 0 | Draw2DSnap 0x462FF0 | axis-aligned, ignores rotation. x range = [pos.x − hw, pos.x + hw] (or [pos.x, pos.x + 2hw] if anchor-left bit 11); y likewise with bit 12. **Pixel snapping**: after adding the global offset, x and y are rounded to nearest (x87 round-to-nearest-even) and 0.5 subtracted |
| 1 | Draw2D 0x463800 | as mode 0 but no snapping |
| 2 | Draw2DRot 0x463210 | if rotation.z == 0 → exactly mode 0 (with snapping). Else corners (±hw, ±hh) rotated by rotation.z: `x' = dx·cos − dy·sin + pos.x`, `y' = dx·sin + dy·cos + pos.y`; then if bit 11 add hw to all x, if bit 12 add hh to all y (offset not rotated) |
| 3 | Draw2DRotNoSnap 0x463470 | as mode 2 but the rotation.z == 0 case is computed inline without snapping |
| 4 | DrawBillboard 0x463CF0 | projects pos through the current view/projection/viewport (D3DXVec3Project, world = translation(pos)); culled if projected z ∉ [0,1]; half-size = 0.5·|project(ref) − project(origin)|·sprite_w·scale.x (y likewise) where `ref` = 0x4EA3F4 = `g_Stage`+0x63C4 = normalise(cross(camera facing, up)), the camera **right** vector written by `Stage_SetCamera` 0x40B6D0 every frame (correction: earlier text said it was never written) — i.e. half-size = half the on-screen length of one world unit along camera-right × sprite size × scale; rotation z applied; vertex z = projected z. No script selects mode 4 itself; the effect manager (list B) calls it directly (13 §2.4). **Reachability (RE-Q25, VERIFIED):** `draw_mode` (+0x200) is written only by ANM op 83 (and VM init = 0); op 83 appears 56 times in the shipped files, always with value 2. `AnmManager_DrawByMode` itself has one caller, `Player_DrawBulletsHit`. So modes 4 and 5 are never reached through the dispatch; `DrawBillboard` is called directly by `EffectMgr_Draw` / `EffectMgr_DrawBackground`, `Draw3DQuad` directly by `EffectMgr_DrawBackground` (effect layer byte +0x354 ∉ {1, 4}) and `Hud_DrawBoss` (12 §5.4/§5.7) |
| 5 | Draw3DQuad 0x464070 | world matrix (rebuilt only when bit 2 or 3 is set and bit 16 is clear; clears bits 2,3) = base_matrix with row 0 ·scale.x, row 1 ·scale.y, then × RotationX(rot.x) × RotationY(rot.y) × RotationZ(rot.z) (each only if ≠ 0); translation = pos (+hw / +hh if anchor bits 11/12). The unit-quad corners (−128,−128,0), (128,−128,0), (−128,128,0), (128,128,0) are projected (D3DXVec3Project with current viewport, projection, view) to screen and emitted as a 2D quad (projected z used as vertex z — `EmitQuad` never rewrites z; with the default 2D camera a z = 0 VM projects to depth ≈ 0.9994, VERIFIED golden draw log chunk 5: capture.anm stage-transition quads at z 0.9994, RE-Q25) |

Every emitted quad then goes through `AnmManager_EmitQuad` (0x4628B0):
1. Add the global screen offset (AnmManager +0x1C, +0x20; written by the screen-shake effect) to all x, y;
   snapping applied here for mode 0.
2. UVs: `u0 = sprite.u0n + uv_off_u`, `u1 = sprite.u1n + uv_off_u`, `v0 = sprite.v0n + uv_off_v`,
   `v1 = sprite.v1n + uv_off_v` (normalised sprite rect, 03_images.md); v0 gets (u0,v0), v1 (u1,v0),
   v2 (u0,v1), v3 (u1,v1).
3. Cull: draw only if the quad's bounding box intersects the current viewport rectangle.
4. Texture of the sprite's entry; colour = colour (or colour2 if bit 17), optionally multiplied by a global
   tint (AnmManager+4 ≠ 0: per channel `min(255, c·tint >> 7)`), written to all four vertices
   (or to D3DRS_TEXTUREFACTOR in the texture-factor colour path chosen by a config flag).
5. Render state from flags: blend (bits 4–5), z-write (bit 13), camera (bit 15 → stage camera, else default 2D
   camera). Quads with equal state are batched; a state change flushes the batch.

Global render state set once at device init (VERIFIED): SRCBLEND = SRCALPHA, DESTBLEND = INVSRCALPHA (then per
blend bits), alpha test on with ALPHAREF = 4 and ALPHAFUNC = GREATEREQUAL (texels with alpha < 4 are discarded),
ZFUNC = ALWAYS (the stage draw temporarily uses LESSEQUAL), vertex fog LINEAR (off except for the stage).
Texture stage: colour and alpha = texture × diffuse (or × texture factor).

Default 2D camera (`Camera_Set2D` 0x40B5A0) used by mode 4/5 when bit 15 is clear — VERIFIED constants:
fovY = 0.31415927 (π/10), aspect = vp.w / vp.h, near = 1, far = 10000,
eye = (vp.w/2, vp.h/2, (vp.h/2)/tan(fovY/2)), at = (vp.w/2, vp.h/2, 0), up = (0, −1, 0), left-handed.
With this camera, world (x, y, 0) projects to viewport pixel (x, y) (relative to the viewport origin).

## 10. Floating-point environment — VERIFIED (runtime, phase 8)

- The D3D8 device is created without D3DCREATE_FPU_PRESERVE (behaviour flags 0x40 or 0x20, VERIFIED statically).
  Runtime measurement (changelog/answers_phase8.md §1): x87 control word 0x027F before `CreateDevice`, **0x007F** after it,
  and 0x007F at every Frame_Run calc/draw/Present on the main thread. 0x007F = all exceptions masked,
  precision control 00 = **24-bit mantissa (single)**, rounding control 00 = **round to nearest even**.
  No game code executes `fldcw` (byte scan of 0x402000..0x476CE0); only CRT helpers (ftol, math) change it
  temporarily and restore it.
  So every x87 arithmetic result on the main thread is rounded to a float32 mantissa, including intermediates
  kept in registers. The exponent range stays extended, but gameplay values never get near it. All VM
  arithmetic above is therefore IEEE float32 arithmetic, one rounding per operation.
- Exception: loader threads (stage start, menu loader) run with **0x027F (53-bit)**, see 11 §1.2.
- float → int conversions (`crt_ftol`) truncate toward zero (VERIFIED from its code).
- sin/cos in draw code are x87 `fsin/fcos` (`FSINCOS`). Their results are rounded to 24 bits under this CW.

## 11. Verification

- `python3 -I tools/anmdis.py extracted/dat -q` → coverage line lists ops −1..89 actually used, "unknown
  opcodes: []", "problems: 0" (sizes, jump targets, script contiguity). VERIFIED.
- Every handler in the table was read from the switch in `AnmVm_Run` (case value = opcode + 1).

## 12. Implementer questions answered in Phase 3 (VERIFIED unless marked)

### 12.1 Global tint (AnmManager +0 / +4)
The tint is a **D3DCOLOR stored at AnmManager+0** (byte +0 B, +1 G, +2 R, +3 A); the dword at **+4 is only the
enable flag** (≠ 0 → tint active). When enabled, every channel **including alpha** is multiplied:
`c' = min(255, (c · t) >> 7)` (0x80 = neutral, helper 0x462750), applied after choosing colour/colour2.
Both colour paths do it (vertex colour in `AnmManager_EmitQuad`, texture-factor colour in
`AnmManager_SetRenderState` 0x4623C0).

### 12.2 Draw-skip test and colour2
All draw entries (modes 0–5, `Draw3DWorld`, `DrawCustomQuad`) skip the VM when flag bit 0 (enabled) or bit 1
(visible) is clear, or when the **primary colour's alpha (byte vm+0x1F3) is 0** — this is tested even when bit 17
(draw with colour2) is set; alpha2 (vm+0x1F7) is never tested.

### 12.3 Op 59 (`irand`) with n = 0
`Rng_Range(n)` returns 0 **without touching the RNG** when n == 0 (the argument is treated as unsigned;
otherwise `Next32 % n`). The RNG algorithm/seed belongs to spec 11 (deferred).

### 12.4 Device render state, texture filtering and addressing (0x442F60, run at device init and after a reset)
Render states: ZENABLE 1 (0 if a config option is set), LIGHTING 0, CULLMODE NONE, ALPHABLENDENABLE 1,
SHADEMODE GOURAUD, SRCBLEND SRCALPHA, DESTBLEND INVSRCALPHA, ZFUNC ALWAYS, ALPHATESTENABLE 1, ALPHAREF 4,
ALPHAFUNC GREATEREQUAL, FOGENABLE 1 (0 by option), FOGDENSITY 1.0, FOGTABLEMODE NONE, FOGVERTEXMODE LINEAR,
FOGCOLOR 0xFFA0A0A0, FOGSTART 1000, FOGEND 5000, EDGEANTIALIAS 0, MULTISAMPLEANTIALIAS 0.
Texture stage 0: COLOROP MODULATE (SELECTARG1 when config flags +0x150 bit 8), COLORARG1 TEXTURE,
COLORARG2 TFACTOR (DIFFUSE when config bit 1 — the "vertex colour" path), ALPHAOP / ALPHAARG1 / ALPHAARG2 the
same way; **ADDRESSU = ADDRESSV = WRAP**, **MAGFILTER = MINFILTER = LINEAR**, MIPFILTER NONE,
TEXTURETRANSFORMFLAGS COUNT2, ADDRESSW CLAMP. Then the AnmManager state caches are invalidated
(blend +0x24C0 = 3, z-write +0x24C1/+0x24C3 = 0xFF, vertex path +0x24C2 = 0xFF, texture +0x24BC = 0,
camera +0x24C4 = 0xFF). No other code changes filter or address mode.

**Consequence for snapped 2D sprites** (changelog/answers_bugs2.md §C1). The original renders at 640×480. Mode 0 / unrotated
mode 2 quads at scale 1 are snapped to integer − 0.5. So with D3D8 pixel-centre rules every pixel centre lands
**exactly on a texel centre** of the sprite rect. Bilinear filtering then returns that texel unchanged: no texel
outside the sprite rect is ever read, even though neighbouring atlas sprites touch it. The result is identical to
point sampling. A reimplementation that renders at a higher resolution, or without the −0.5/half-pixel
correspondence, still with LINEAR filtering, blends the row/column just outside the rect into the quad edge. That
shows up as thin lines at sprite edges (e.g. the main-menu items in `title01.png`, whose glow rows run on into the
neighbouring sprites). To match: render 2D at 640×480 and scale the final image, **or** sample snapped, unscaled 2D
quads with nearest filtering, **or** inset the UVs by half a texel. 3D/rotated/scaled quads do sample neighbours
in the original too (06 §seams).

### 12.5 Render state at frame start
The per-frame "reset" (0x462E00, called right after BeginScene) only empties the vertex batch; it does **not**
touch any D3D state or the caches. So each frame starts with whatever blend / z-write / camera / texture the
last draw of the previous frame left (the caches still describe it correctly). Blend bits 4–5 of a VM:
0 → DESTBLEND INVSRCALPHA, 1 → DESTBLEND ONE (additive); 2 and 3 set nothing (the previous blend stays).
Fog is switched off by the frame loop before the draw chain (`Supervisor_FogOff`).

### 12.6 Supervisor flag 0x20 (slowdown restore)
Set (Supervisor+0x1A4 = 0x17CE8FC |= 0x20) by ECL native 29 (0x425290) when it restores normal speed after a
slowdown 1/n with n > 1. **No code clears it.** While set, every `Timer_Add`/`Timer_Sub` (and so every
`Timer_Decrement`, used by bullets in spawn states, item/player countdowns, ECL waits, …) first moves the counter
one extra step in the same direction (+1 for Add, −1 for Sub), zeroes the sub-frame and sets prev = −999;
`Timer_Tick` is not affected. (Faithful quirk; INFERRED: practically only reachable after a native-28/29 pair.)

### 12.7 Draw mode 5 / `Draw3DWorld` anchor offset
With anchor bit 11 (x) / 12 (y) the translation gets `+ |sprite_w · scale.x / 2|` / `+ |sprite_h · scale.y / 2|`
— the **absolute value** (FABS) of the half size, so negative scales still shift by a positive amount.
The global screen-shake offset is added afterwards.

### 12.8 Texture-matrix / texture caches
`AnmManager+0x24C8` (last sprite record used for the texture matrix) is used only by `Draw3DWorld`; it is reset
to 0 at the start of `Stage_DrawLow` every frame and by a few other drawers (ending 0x429980, 0x43741D,
screen effects 0x45B1E0/0x45B490). The texture cache `+0x24BC` **is shared** with the 2D paths
(`EmitQuad`/`DrawCustomQuad` compare and update the same field); `Draw3DWorld` re-binds the texture only when
its sprite cache misses (or uv_off_u ≠ 0), and the same reset points clear +0x24BC too. Quirk (see 06 §11.2):
the update condition tests uv_off_u twice and never uv_off_v.
