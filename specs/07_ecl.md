# 07 — Enemy scripts (.ecl): format, VM, enemies, opcodes

Source: th08.exe 1.00d. Interpreter `Ecl_Run` 0x4184B0, timeline runner `EclTimeline_Run` 0x42A8A0, enemy
manager calc `EnemyMgr_Calc` 0x42C660 (calc prio 11), helpers named in `symbols.csv`. Tool:
`tools/ecldis.py` disassembles every sub and timeline of all 24 `.ecl` files (`extracted/ecl_dis.txt`):
all opcodes known, every instruction size matches its signature, every jump lands on an instruction, every
sub ends with the −1 marker, every literal sub id is valid → "problems: 0".

**VERIFIED** = read from code and consistent with data. **INFERRED** = interpretation / naming. Offsets of
enemy fields are given as `e+0x…` (enemy object, 0x53D0 bytes). Subsystems outside ECL (bullets, items,
effects, HUD, player, spell-card bonus) are only described as far as ECL drives them; their internals belong
to later specs.

## 1. File selection (VERIFIED, `EnemyManager_Load` 0x42EBF0)

Normal play: `ecldataN.ecl` by stage index (04_file_index.md). Spell-practice mode (global flag 0x164D0B4
bit 14): `ecldataNsp.ecl`, except spell ids ≥ 205 (last words) which use per-character files
`ecldata_{xx}.ecl` from the table at 0x4C70E4 indexed by spell id. (`stgNenm.anm` likewise.)

## 2. File layout (VERIFIED, loader 0x418330)

| off | type | meaning |
|---|---|---|
| 0x00 | u32 | 0x800 (checked; load fails otherwise) |
| 0x04 | i16 | number of subs |
| 0x06 | i16 | number of timelines **run** (`ntl`) |
| 0x08 | u32[16] | timeline offsets (0 = none). Data always has `ntl + 1` non-zero entries; the last one is never run |
| 0x48 | u32[nsubs] | sub offsets |

All offsets are from file start (relocated to pointers at load).

### 2.1 Sub instruction (VERIFIED)

```
+0  i32 time        instruction executes when the context clock == time (exact equality, §5.3)
+4  i16 opcode      -1 = end-of-sub marker (never executed; every sub ends with it)
+6  i16 size        total size; next = this + size
+8  u8  (unused by the interpreter; always 0 in data)
+9  u8  rank mask   (§2.3)
+10 u16 var mask    bit k = argument k is a variable id (§6). Bullet/laser ops use their own bit layout (§9)
+12 args            4 bytes each
```

### 2.2 Timeline instruction (VERIFIED)

```
+0 i32 time (< 0 = end of timeline)   +4 i16 opcode   +6 u8 size   +7 u8 rank mask   +8 args
```

### 2.3 Rank masks (VERIFIED)

`D = (difficulty < 4 ? 1 << difficulty : 0x0F) | e.mode_bits` where difficulty = global 0x160F538
(0 Easy … 3 Lunatic, 4 Extra) and `e.mode_bits` (e+0x3330) is 0 for ordinary enemies and 0x20 (player in
human mode) / 0x40 (youkai mode) for enemies with flag bit 8 (§10.1).
- Sub instruction runs iff `(mask & D) == D`; otherwise it is skipped (ip advances, same frame).
- Timeline instruction runs iff `(mask & (1<<difficulty or 0x0F)) != 0`.
Data uses masks 0xFF, 0xF1/F2/F4/F8 (single difficulty), 0xF3, 0xFC, 0xFE, 0xF7, 0x3F/0x2E (human only),
0x5F/0x4E (youkai only).

## 3. Timelines (VERIFIED, `EclTimeline_Run`)

Each timeline i < ntl has {Timer clock, instruction pointer} (enemy manager +0x9DCDD0 + 16·i), started at its
offset when the stage starts. Every frame (calc, before enemies are updated):

```
loop:
  ins = ptr
  if ins.time < 0: clock.Tick(); return
  if clock == ins.time: run ins (if rank mask matches)
  elif clock < ins.time: clock.Tick(); return
  (clock > ins.time: instruction is skipped)
  ptr += ins.size
```
(Unlike subs, late timeline instructions are skipped, not stalled.)

| op | name (assigned) | args | effect | uses |
|---|---|---|---|---|
| 0 | spawn | sub, f x, f y, i life, i item, i score | spawn enemy at (x, y, 0) — unless a slot-0 boss is registered (HUD +0x2C "boss display" byte, 0x42F320; set by op 127 slot 0, cleared when a boss slot < 4 unregisters) or spawning is disabled (global 0xF54E2C, ECL op 175) | 968 |
| 1 | spawn_mirror | same | same with the enemy's **mirror** flag (bit 18): all its x motion is negated | 625 |
| 2 | spawn_randx | sub, f x0, f x1, f y, life, item, score | x = Rng_Float·(x1 − x0) + x0 | 115 |
| 4 | spawn_randx_mirror | same | mirrored | 37 |
| 3 / 5 | spawn_randx_full (/mirror) | sub, f y, life, item, score | x = Rng_Float·384 | 0 |
| 6 | msg | i n | start dialogue n (HUD 0x439810; 10_msg later) | 35 |
| 7 | wait_msg | — | while a dialogue runs: clock.Decrement() and stop this frame (stall) | 43 |
| 8 | interrupt_boss | i slot, i n | boss[slot].pending_interrupt = n (§5.5) | 18 |
| 9 | set_power | i n | `GameMgr_SetPower`(n) (0x406FA0; power := n, checksum with RNG, 09 §9.4) | 0 |
| 10 | wait_boss | i slot | while boss[slot] exists and is alive: stall | 52 |
| 11 / 12 | spawn_items (/mirror) | sub, f x, f y, i life, i a, i b, i score | spawn with item = −1; then e+0x3308 = a (point items), e+0x330C = b (power items) | 0 |
| 13 | wait_sync | i v | look in the 4 sync slots (mgr+0x9DCEFC, init −1): every slot == v is consumed (set −1); if none matched: stall | 46 |
| 14 | post_sync | i v | every sync slot < 0 is set to v | 48 |
| 15 | spawn_force | as 0 | spawn without the boss/disable checks (not mirrored) | 1 |
| 16 | flag | — | global byte 0x164D0BB = 1 (later phase) | 15 |

"Stall" = `clock.Decrement()` immediately followed by the normal `clock.Tick()` exit, i.e. net zero: the clock
keeps its value (also at game speed < 1, the sub-frame step is undone) and the same instruction is re-tested next
frame; instructions before it in the same frame are not re-run. VERIFIED (0x42A8A0).

## 4. Enemy objects (VERIFIED unless marked)

Enemy manager (0x577F20): a **template enemy** at +0 (defaults, §4.2), 480 enemy slots at +0x53D0
(stride 0x53D0), boss table 0xF54CC0[4] (= mgr+0x9DCDA0), timelines, sync slots.

### 4.1 Spawning (`EnemyMgr_SpawnFromTimeline` 0x42A4E0 / `_FromEcl` 0x42A680)
1. First slot whose alive bit (e+0x3324 bit 0) is clear; none → nothing (manager "full" flag set).
2. Copy the template into the slot; e+0x2E0C = slot index; mirror bit 18 = mirror; if life ≥ 0: life = life;
   pos = spawn position; main context set to `sub` (ip = sub start, clock 0, wait 0).
3. ECL-spawned enemies (ops 90–94) also copy the parent context's 30 variable dwords (ctx+0x18…+0x8F,
   i.e. vars 10000–07, 10016–23, 10036–39, 10094–95, 10053–60) into the child's main context.
4. **Run `Ecl_Run` once immediately** (its time-0 instructions). If it returned −1 (op 1) the slot is freed.
5. Otherwise: e+0x2E20 = main ANM VM colour; item (e+0x3304) = item (signed char); score (e+0x2E08) = score
   if ≥ 0; max life (e+0x2E00) = e+0x2E04 = life (ECL spawn also re-applies life ≥ 0 after the run).

### 4.2 Template defaults (`EnemyMgr_Ctor` 0x429E00)
alive (bit 0), bits 2, 3, 6 set; life 1; score 100; hit box (e+0x2D70) = (24, 24, 24); velocity, angle, speed,
accel, angular velocity 0; move mode 0; no mirror; death sub −1; pending interrupt −1; life thresholds −1;
timeout −1; directional anims −1; shot offset 0; auto-fire interval 0; bullet-rank ranges speed −0.15…0.15,
counts 0; bullet sfx 7, desc+0x204 = 25; **no-fire radius² 1024** (no bullets within 32 px of the player);
sub-ANM VMs disabled; e+0x3330 = 0.

### 4.3 Field map (main ones)

| off | type | meaning |
|---|---|---|
| 0x004 / 0x008 | ptr | chain prev / next (child chain, §10.4) |
| 0x00C | AnmVm | main sprite VM |
| 0x2B0, 0x554 | AnmVm[2] | sub-sprite VMs (op 57/61) |
| 0x7F8 | Context | main script context (0x228 bytes, §5.1) |
| 0xA20 | Context[16] | call stack |
| 0x2CA0 / 0x2CA4 | ptr | current context / current stack base |
| 0x2CA8–0x2CC4 | i32[8] | enemy int vars 10008–10015 |
| 0x2CC8–0x2CE4 | f32[8] | enemy float vars 10024–10031 |
| 0x2CE8 / 0x2CEA | i16 | saved main stack depth / current depth |
| 0x2CEE | i16 | death sub (op 130), −1 none |
| 0x2CF0 | i16[32] | interrupt table: sub id per interrupt index (op 126) |
| 0x2D30 | i16 | pending interrupt index (−1 none) |
| 0x2D34 | f32[3] | position |
| 0x2D40 | f32[3] | position offset (parent position for followers) |
| 0x2D4C | f32[3] | velocity (per frame at game speed 1) |
| 0x2D58 / 0x2D64 | f32[3] | previous position / last frame's displacement (vars 10085–87) |
| 0x2D70 / 0x2D7C | f32[3] | hit box (op 77) / extra shot box (op 78; used if x > 0) |
| 0x2D88 | f32[3] | final position = pos + offset (z forced 0 after movement) |
| 0x2D94 / 0x2D98 | f32 | movement angle / angular velocity |
| 0x2D9C / 0x2DA0 | f32 | orbit angle / orbit angular velocity |
| 0x2DA4 | ptr | parent enemy (chain root) |
| 0x2DA8 / 0x2DAC | f32 | speed / acceleration |
| 0x2DB0 / 0x2DB4 | f32 | orbit radius / radius velocity |
| 0x2DB8 | f32[3] | shot offset (op 110) |
| 0x2DC4 / 0x2DD0 | f32[3] | interpolation delta / start (or orbit centre) |
| 0x2DDC / 0x2DE8 | Timer / i32 | movement timer / duration |
| 0x2DEC–0x2DFA | f32,f32,i16×4 | bullet rank ranges (op 152) |
| 0x2DFC / 0x2E00 / 0x2E04 | i32 | life / max life / life for HUD |
| 0x2E08 | i32 | score |
| 0x2E0C | i32 | slot index |
| 0x2E14 | Timer | boss/spell timer (op 132), ticks every frame unless global 0x160F534 ≠ 0 |
| 0x2E24 | desc | bullet spawn descriptor (§9; reset from defaults at 0x57AD44) |
| 0x3034 | 44 B | stored bullet instruction for auto-fire |
| 0x3060 / 0x3064 | i32 / Timer | auto-fire interval / timer |
| 0x3070 | desc | laser descriptor |
| 0x3280 | ptr[32] | laser slots; 0x3300 = current slot index |
| 0x3304 | i32 | item drop (var 10092) |
| 0x3308 / 0x330C | i32 | extra point / power items on death |
| 0x3310–0x3312 | u8 | death-effect parameters (op 138) |
| 0x3313 | u8 | boss slot (var 10084) |
| 0x3324 | u32 | flags A (§4.4) |
| 0x3328 | u32 | flags B (§4.4) |
| 0x332E / 0x332F | i8 / u8 | current direction anim / draw layer |
| 0x3330 | u8 | rank-mask mode bits |
| 0x3332–0x333C | i16 | direction anim scripts (op 55/56) |
| 0x3340–0x334C | f32×4 | clamp rect xmin, ymin, xmax, ymax |
| 0x3350 | f32 | no-fire radius² |
| 0x3354 | i32 | (var 10083) |
| 0x3358 / 0x3368 | i32[4] | life thresholds / their subs (op 133) |
| 0x3378 / 0x337C | i32 | timeout frames / timeout sub (op 134) |
| 0x3380 | i32 | chained children created (ops 90–92 +1) minus children killed by damage while chained (`EnemyMgr_Calc` −1); never reset by a chain kill (answers_impl12 §2) |
| 0x3384 | ptr[4] | extra script threads (op 135) |
| 0x5354 | Timer | damage-reduction timer (op 160) |
| 0x5360 / 0x53C0 | ptr[] / i32 | effect handles (op 128) / count; 0x53C4 = effect parameter |
| 0x53C8 | ptr | aura effect (ops 90–92, 174) |
| 0x53CC | i32 | spell-capture bonus seconds |

### 4.4 Flag bits

Flags A (e+0x3324): 0 alive · 1 boss · 2 contact hurts player · 3 loses life when hit · 4 not drawn, no screen check, no collision · 6 collides
with player shots · 7 bonus on cancel (item 6 + popup in kill_all; special player interaction) · 8 reacts to
player human/youkai mode · 9 follows parent position · 10 chained child being killed · 11 player in youkai
mode (copied) · 12–13 movement mode · 14–16 movement easing · 17 store bullets for auto-fire · 18 mirror X ·
19 clamp to rect · 20–22 death mode · 23 dead-but-continuing · 24 has been on screen · 25, 30, 31 (op 145,
173, 183; later phase) · 26 calls do not push the stack · 27 spell bonus fixed at 99 999 990 · 28 not deleted
when leaving the screen · 29 skip movement step. (VERIFIED as bit usage; names of 23/25/30/31 INFERRED.)
Flags B (e+0x3328): 1 (op 83) · 2 sprites from stage enemy ANM (`stgNenm.anm`) instead of `enemy.anm` ·
3 death latch · 4–5 low-life warning level · 6 cannot die · 7 paused (native 12: `EnemyMgr_Calc` does not run the enemy's ECL and holds its script time with
`Timer_Decrement(e+0x2E14)`; the same skip happens when flags A bit 30 is set and the player is bombing or player byte
0x17D5EF8 ≠ 0; answers_impl10 §4) · 8 (op 182).

## 5. Script VM

### 5.1 Context (0x228 bytes) — VERIFIED

| off | meaning |
|---|---|
| 0x00 | ip |
| 0x04 | Timer clock |
| 0x10 / 0x14 | per-frame native callback (op 137) and its instruction |
| 0x18–0x34 | int vars 10000–10007 |
| 0x38–0x54 | float vars 10016–10023 |
| 0x58–0x64 | int vars 10036–10039 |
| 0x68–0x6C | float vars 10094–10095 |
| 0x70–0x7C / 0x80–0x8C | int vars 10053–56 / float vars 10057–60 (call arguments, §5.4) |
| 0x90 | Timer wait (op 2) |
| 0x9C | 8 variable-interpolation slots × 0x30 (op 36) |
| 0x220 | thread index + 1 (threads only) |
| 0x224 | i16 current sub id |

A thread (op 135) is a 0x24B0-byte block: +0 sub id, +6 i16 stack depth, +8 context, +0x230 its own
16-entry call stack.

### 5.2 `Ecl_Run(e)` per frame — VERIFIED

```
for ctx in [main, thread0..thread3 (non-null, in order)]:
  LOOP:
    if e.pending_interrupt >= 0: INTERRUPT (§5.5); goto LOOP
    EXEC:
      e.final_pos = e.pos + e.pos_offset
      if ctx.wait > 0: ctx.wait.Decrement(); ctx.clock.Decrement(); goto POST
      ins = ctx.ip
      if ctx.clock != ins.time: goto POST
      if rank mask fails: ip += size; goto EXEC
      execute (§7); unless the op transfers control: ip += size; goto EXEC
    POST:
      if e.life > 0:
         if ctx.callback: callback(e, ctx.callback_instr)
         run interpolation slots (§5.6)
      ctx.clock.Tick()
return 0 (or -1 when op 1 executed: the caller frees the enemy)
after all contexts: Enemy_UpdateMoveMode (§8), Enemy_AutoFireAndDirAnim (§9.3)
```
- **Exact time match**: an instruction only runs on the frame the clock equals its time. Jumps set the clock
  to the target time explicitly. An instruction whose time is already past is never reached (the sub stalls).
- Threads created this frame by the main context run in the same frame (they are visited after main).

### 5.3 Clock
`Timer` semantics as in 05_anm_vm.md §2 (game-speed aware Tick/Decrement).

### 5.4 Calls, returns, threads — VERIFIED
- **call (op 52, `Ecl_CallSub` 0x421BD0)**: ctx.ip = next instruction; unless flag A bit 26: push a copy of
  the whole context onto the stack (stack[depth]); set the context to the sub (ip = start, clock = 0, wait = 0,
  sub id); copy the 8 global "argument" dwords (vars 10061–10068, globals 0x4ECE20–0x4ECE3C) into the
  callee's vars 10053–10060; depth += 1 (max 15, only when pushing). Execution continues immediately.
- **return (op 53, 0x421CB0)**: depth −= 1; if depth < 0 (a thread returning from its first sub): free the
  thread, continue with the next context; else restore the whole context from stack[depth] (ip, clock, local
  vars …) and continue.
- **thread (op 135 slot, sub)**: free thread slot; if sub ≥ 0 create it (copy of the current context's 30
  var dwords, clock 0).
- Op 88 calls a sub *in another enemy's* (boss slot) main context the same way.

### 5.5 Interrupts — VERIFIED
`e.pending_interrupt = k` (set by op 125 immediately, op 89 / timeline op 8 on another enemy). At the next
LOOP check: ctx.ip = ip + size of the **current** (not yet executed) instruction — i.e. that instruction is
skipped when the interrupt returns; unless flag A bit 26: push the context; set sub = interrupt_table[k]
(e+0x2CF0, op 126); depth += 1 (≤ 15); pending = −1.

### 5.6 Variable interpolation slots (op 36) — VERIFIED
Slot = {fn, Timer elapsed, dur, fn index, easing mode, a, b, c, d, target var id}. Op 36 reuses the slot whose
target equals the new target, else the first free slot (8 max; ignored when full). Each POST:
`elapsed.Tick(); if elapsed ≥ dur: elapsed = dur; t = elapsed/dur; t = ease(mode, t)` (1 t², 2 t³, 3 t⁴,
4 1−(1−t)², 5 1−(1−t)³, 6 1−(1−t)⁴, else t); `fn(t)`; slot freed when elapsed ≥ dur.
fn index 0–6: `var = a + (b − a)·t`; 7: Hermite `var = (2t+1)(t−1)²·a + (3−2t)t²·b + t(1−t)²·c + (t−1)t²·d`.
The stored a–d are re-read through the float-variable getter at every evaluation. If any slot targets
10042/10043/10044 (enemy position), after all slots: velocity.xy = new pos − old pos, angle =
atan2(vel.y, vel.x), and the position is restored (the movement step then applies the velocity).

## 6. Variables — VERIFIED (getters 0x41F420 / 0x420120, references 0x41FE10 / 0x420950)

If a mask bit is set the argument is a variable id (ints as int32, floats as float32 holding e.g. 10016.0,
truncated to int before lookup). Int read of a float variable truncates; float read of an int variable converts.
Destination arguments that are not a valid writable variable of the right type write **into the instruction's
own argument slot in the loaded file** (shared by all enemies; e.g. op 5 with a literal counter).

| id | int / float read | writable as | meaning |
|---|---|---|---|
| 10000–10007 | ctx ints | int | context local ints |
| 10008–10015 | enemy ints | int | per-enemy ints (shared by all its contexts) |
| 10016–10023 | ctx floats | float | context local floats |
| 10024–10031 | enemy floats | float | per-enemy floats |
| 10032 | Rng_Next32 & 0x7FFFFFFF | — | random |
| 10033 | Rng_Float ∈ [0,1) | — | random (int read: 0) |
| 10034 | Rng_Next32 (signed) | — | random |
| 10035 | Rng_Next32/2³¹ − 1 ∈ [−1,1) | — | random |
| 10036–10039 | ctx ints | int | (10038 also = laser test result, op 120) |
| 10040 | difficulty | int | global 0x160F538 |
| 10041 | rank (GM+0x3DE2C; per-difficulty range, e.g. 8–16; changed via a 1/100 sub-counter, 11 §3.2) | int | global 0x164D334 (§9.2) |
| 10042–10044 | enemy pos x, y, z | float | |
| 10045–10047 | player pos x, y, z | float (!) | global 0x17D61AC |
| 10048 | angle enemy final pos → player (π/2 if coincident) | — | |
| 10049 | e+0x2E1C | int | |
| 10050 | distance to player | — | |
| 10051 | life | int | |
| 10052 | player character (byte 0x164D0B1) | — | |
| 10053–10056 / 10057–10060 | ctx call-argument ints / floats | int / float | |
| 10061–10064 / 10065–10068 | global argument ints / floats | int / float | passed to the next call |
| 10069 / 10070 | angle / angular velocity | float | |
| 10071–10073 | speed, accel, orbit radius | float | |
| 10074–10076 | interpolation start / orbit centre | float | |
| 10077 / 10078 | orbit angle / orbit angular velocity | float | |
| 10079–10081 | interpolation delta | float (int read: literal) | |
| 10082 | random angle Rng_Float·2π − π | — (float only) | |
| 10083 | e+0x3354 | — | |
| 10084 | boss slot | — | |
| 10085–10087 | last-frame displacement | — | |
| 10088–10091 | life thresholds 0–3 | — | |
| 10092 | item drop | int | |
| 10093 | score | int | |
| 10094 / 10095 | ctx floats | float | |
| 10096 | depth of this enemy in its child chain (0 if not chained) | — | |
| 10097 | player mode byte (0x17D5EF8+5) | — | |
| 10098 | (int only) 2 if a+b+c < d else 0 (game counters; later phase) | — | |
| 10099 | spell-card state (bomb-allowed / running) | — | |
| 10100 | (int only) spell-card timer | — | |

## 7. Opcode table — VERIFIED (switch in `Ecl_Run`; uses = count in all files)

Arg letters: `i` int, `f` float, `V`/`W` int/float variable destination, `T` time, `O` byte offset relative to
**this instruction**, `S` sub id, `raw` = not variable-capable. "mode1" etc. refer to §8.

| op | name | args | effect | uses |
|---|---|---|---|---|
| 0, 3, 84, 85 | nop | (3: one ignored arg) | nothing | 278 / 8 / 0 / 0 |
| 1 | delete | — | Ecl_Run returns −1 → enemy freed immediately (no drops/score) | 519 |
| 2 | wait | i n | ctx.wait = n (§5.2) | 530 |
| 4 | jump | T t, O off | clock = t; ip = this + off | 1005 |
| 5 | loop | T t, O off, V c | c −= 1; if c > 0: jump | 575 |
| 6 / 7 | iset / fset | V/W a, b | a = b | 3954 / 3816 |
| 8 / 9 | irandsign / frandsign | V/W a, b | a = (Rng_Next16 & 1) ? b : −b | 0 / 38 |
| 10–14 | iadd isub imul idiv imod | V a, i b | a op= b (C semantics: truncating /, %) | 105 48 12 8 0 |
| 15–19 | fadd fsub fmul fdiv fmod | W a, f b | a op= b; fmod(a, b) | 2239 442 325 30 0 |
| 20–24 | iadd3 … imod3 | V a, i b, i c | a = b op c | 2 14 2 28 64 |
| 25–29 | fadd3 … fmod3 | W a, f b, f c | a = b op c | 355 263 499 128 0 |
| 30 / 31 | inc / dec | V a | a ± 1 | 81 / 25 |
| 32 / 33 | fsin / fcos | W a, f b | a = sin b / cos b | 136 / 131 |
| 34 | fatan2 | W a, f x1, y1, x2, y2 | a = atan2(y2 − y1, x2 − x1) | 38 |
| 35 | flerp | W a, f b, f c, f d | a = (b − c)·d + c | 0 |
| 36 | interp | raw var, i dur, i fn, i mode, f a, b, c, d | start interpolation slot (§5.6) | 97 |
| 37 | normangle | W a | a = wrap(a) into [−π, π] | 1708 |
| 38 | polar | W x, W y, f ang, f r | x = cos(wrap ang)·r; y = sin(wrap ang)·r | 110 |
| 39 | fdist | W a, f x1, y1, x2, y2 | a = √((x1−x2)² + (y1−y2)²) | 2 |
| 40–51 | ije fje ijne fjne ijl fjl ijle fjle ijg fjg ijge fjge | a, b, T t, O off | if a (==, ≠, <, ≤, >, ≥) b: clock = t; ip = this + off | 93 0 94 0 28 11 35 31 95 6 70 139 |
| 52 | call | S sub | §5.4 | 614 |
| 53 | return | — | §5.4 | 250 |
| 54 / 58 | anm / anm_stg | i script | main VM: start script from `enemy.anm` (54, clears flags B bit 2) or `stgNenm.anm` (58, sets bit 2) | 291 / 73 |
| 55 / 59 | anm_dir / anm_dir_stg | i base | direction scripts = base … base+5 (§9.3); same file/flag rule | 12 / 138 |
| 56 / 60 | anm_dir6 / _stg | i×6 | direction scripts explicit (idle, left, right, left→idle, right→idle, replay) | 0 / 0 |
| 57 / 61 | subanm / _stg | i vm (0/1), i script | sub VM: script < 0 → disable it, else start script (file by flags B bit 2; 57 clears, 61 sets it) | 46 / 76 |
| 62 | anm_replay | — | restart the "replay" direction script (e+0x333C) on the main VM | 201 |
| 63 | set_pos | f x, f y | pos = (x, y, 0); clamp (§8) | 96 |
| 64 | move_to | i dur, i ease, f x, f y | mode2: delta = (x, y, 0) − final pos, start = pos, timer = dur, velocity 0 (delta.x negated if mirrored) | 342 |
| 65 | set_dir | f ang, f spd | mode1: angle = wrap(ang), speed = spd, duration 0 | 171 |
| 66 | move_dir | i dur, i ease, f ang, f spd | dur < 1: as 65. Else mode2 with delta = (cos a·spd·dur, sin a·spd·dur, 0), start = final pos | 188 |
| 67 | move_rand | i dur, i ease, f spd | random direction (§8.3) then as 65/66 | 270 |
| 68 | set_dir_aim | f off, f spd | angle = wrap(angle_to_player(pos) + off); speed = spd (mode unchanged) | 0 |
| 69 | move_dir_aim | i dur, i ease, f off, f spd | dur < 1: aimed mode1 with movement duration dur. dur ≥ 1: **same as 66 (not aimed)** | 0 |
| 70 / 71 | set_angvel / set_accel | f v | angular velocity / acceleration; mode1 | 54 / 78 |
| 72 | orbit | i dur, f cx, f cy, f a, f av, f r, f rv | mode3 with centre (cx, cy), orbit angle/vel, radius/vel | 61 |
| 73 | orbit_here | i dur, f a, f av, f rv | mode3: centre = pos, radius 0 | 70 |
| 74 | orbit_vel | i dur, f av, f rv | mode3: only orbit angular / radius velocity | 114 |
| 75 / 76 | clamp_rect / clamp_off | f x0, y0, x1, y1 / — | flag A bit 19 on/off | 157 / 34 |
| 77 / 78 | hitbox / shotbox | f w, f h | e+0x2D70 / e+0x2D7C (x, y) | 363 / 33 |
| 79 | flags_set | i m | bit6 = !(m&1), bit2 = !(m&2), bit3 = !(m&4), bit4 = (m&8), bit28 = (m&16), B bit6 = (m&32) | 16 |
| 80 | flags_off | i m | for each set bit of m: 1 → no shot collision, 2 → no contact damage (aura colour off), 4 → no life loss, 8 → invisible, 16 → keep off-screen, 32 → cannot die | 702 |
| 81 | flags_on | i m | inverse of 80 | 535 |
| 82 | nofire_radius | f r | no-fire radius² = r·r | 76 |
| 83 | flagB1 | i b | flags B bit 1 = b & 1 | 195 |
| 86 / 87 | iget_boss / fget_boss | V/W a, i var, i slot | a = value of variable `var` evaluated on boss[slot] (87: only if it exists) | 0 / 2 |
| 88 | call_boss | i slot, S sub | call sub in boss[slot]'s context | 2 |
| 89 | interrupt_boss | i slot, i k | boss[slot].pending_interrupt = k (if exists) | 0 |
| 90 / 91 / 92 | child / child_rel / child_follow | S sub, f x, f y, i life, i item, i score | §10.4 | 214 / 377 / 425 |
| 93 / 94 | spawn / spawn_rel | S sub, f x, y, z, i life, i item, i score | spawn independent enemy at (x,y,z) (94: + pos), vars copied | 9 / 152 |
| 95 | kill_all | — | every alive non-boss enemy without flags B bit 6: life = 0 (dies in its own update), jumps to its death sub if any; enemies with flag A bit 7 also drop item type 6 and a score popup (2000, +30 per enemy, capped at 8000) (0x42EFB0) | 157 |
| 96–104 | bullets | bullet block | §9 (aim mode = op − 96) | 352 546 241 802 10 1 0 0 56 |
| 105 / 106 | autofire / autofire_rand | i n | interval = n + lerp_i(n/5, −n/5) (§9.2); timer = 0 (106: Rng_Range(interval)) | 165 / 44 |
| 107 / 108 | autofire_store on / off | — | flag A bit 17 | 25 / 25 |
| 109 | fire_stored | — | fire the descriptor as currently stored, origin = final pos + shot offset | 0 |
| 110 | shot_offset | f x, f y | (x, y, 0) | 423 |
| 111 | bullet_ex | i k, i a, i b, i c, i d, f e, f f | descriptor extra slot k (e+0x2E44 + 24k) = {f32 e, f32 f, i32 c, i32 d, u32 a, i32 b} (ints stored raw, no float conversion; every argument is variable-capable: mask bit 0 → k, bit 1 → a, bit 2 → b, bit 3 → c, bit 4 → d via the int getter, bits 5/6 → e/f via the float getter — VERIFIED 0x41B565) — per-bullet behaviour commands (08 spec §4) | 1269 |
| 112 | cancel_bullets | — | cancel all bullets/lasers to items (mode 1), 08 §7 | 10 |
| 113 | bullet_sfx | i id, i x | id < 0: descriptor flag 0x200 off; else sfx = id, flag on; desc+0x204 = x | 161 |
| 114 / 115 | laser / laser_b | laser block | §9.4 | 59 / 0 |
| 116 | laser_slot | i k | current laser slot | 18 |
| 117 / 118 / 167 | laser_rotate / aim / set_angle | i k, f a | laser[k].angle = wrap(angle + a) / angle_to_player(laser pos) + a / a | 10 / 0 / 10 |
| 119 | laser_offset | i k, f x, y, z | laser[k].pos = final pos + (x, y, z) | 10 |
| 120 | laser_alive | i k | var 10038 = laser[k] exists and active | 2 |
| 121 | laser_cancel | i k | if active and state < 2: state = 2, timer 0 | 4 |
| 122 | spellcard | spell block | §11 | 431 |
| 123 | spellcard_end | — | spell manager 0x4161B0 | 191 |
| 124 | sfx | i id | play sound effect id at pos | 422 |
| 125 | interrupt_now | i k | pending = k and dispatch immediately (§5.5) | 0 |
| 126 | set_interrupt | S sub, i k | interrupt_table[k] = sub | 52 |
| 127 | boss | i slot | slot < 0: unregister this enemy as boss. Else boss[slot] = e, boss flag, e.boss_slot = slot, no-fire radius 0; slot 0 also turns on the HUD boss display | 307 |
| 128 | effect13 | raw, f a, f b, f c, i d | spawn effect 13 (colour 0xFF6060D0) at pos with params a,b,c; e+0x53C4 = d; kept in e+0x5360[] and updated every frame (follow pos, grow param to d, spin) | 210 |
| 129 | death_mode | raw byte | flag A bits 20–22 (§10.3); ignored in spell practice when 0x164D0B4 bits 7–8 ≠ 0 | 368 |
| 130 | death_sub | raw i16 | e+0x2CEE (same guard) | 357 |
| 131 | set_life | i n | life = max = HUD life = n; boss slot 0: reset 8 HUD markers | 252 |
| 132 | set_timer | i n | e+0x2E14 = n | 153 |
| 133 | life_threshold | i k, i life, S sub | thresholds[k] = life, subs[k] = sub (sub not set under the practice guard) | 113 |
| 134 | timeout | i frames, S sub | timeout = frames, timeout sub = sub (sub not set under guard); timer = 0 | 353 |
| 135 | thread | i k, S sub | §5.4 | 625 |
| 136 | native | i n, i arg | call native function n once (§12) | 182 |
| 137 | native_cb | i n, i arg | per-frame native callback n (n < 0 clears) | 95 |
| 138 | set_bytes | raw | e+0x3310..0x3312 = argument bytes 0..2 | 0 |
| 139 | effect | i id, i n, V/i colour | spawn effect id ×n at pos with colour | 404 |
| 140 | effect_dir | i id, i n, i c, f x, y, z | directional effect | 1093 |
| 141 | item | i type | spawn item at pos | 17 |
| 142 | item_burst | i n | n items at pos + (Rng·128 − 64, Rng·128 − 64): if player power < 128: first type 2, rest type 0; else all type 1 | 22 |
| 143 | set_item | i n | item drop = n | 0 |
| 144 | extra_items | i a, i b | e+0x3308 = a (point), e+0x330C = b (power) | 121 |
| 145 | flagA25 | raw byte | bit 25 | 107 |
| 146 | time_add | i n | ctx clock += n | 0 |
| 147 | std_section | i n | stage section request (06_std §4.1) | 41 |
| 148 | hud148 | i n | HUD 0x423130(n); global 0x164D30C += 1800 (later phase) | 188 |
| 149 | anm_interrupt | i n | main VM pending interrupt = n | 0 |
| 150 | subanm_interrupt | raw vm, raw n | sub VM pending interrupt = n | 0 |
| 151 | no_stack | raw byte | flag A bit 26 | 0 |
| 152 | rank_params | f s0, f s1, i c1a, i c1b, i c2a, i c2b | bullet rank ranges (§9.2) | 34 |
| 153 | timeout_sub_from_death | — | timeout sub = death sub; timer = 0 | 153 |
| 154 | laser_clear | — | forget all 32 laser slots | 0 |
| 155 | fixed_bonus | raw byte | flag A bit 27; global 0x4ECCA8 = 99 999 990 | 19 |
| 156 | flagA7 | raw byte | bit 7; layer byte = 2 | 0 |
| 157 | trail | raw flags, i n, i a, i b | afterimage trail settings (e+0x534C..0x5352; flags & 8 allocates history) | 22 |
| 158 | hp_marker | i k, i a, i b, i c | HUD life-bar marker k at a/max, b/max, style c | 244 |
| 159 | draw_layer | i n | e+0x332F | 38 |
| 160 | dmg_timer | i n | damage-reduction timer (§10.2) | 538 |
| 161 | cancel_radius | f r | bullet manager 0x430D30(final pos, r) (later phase) | 0 |
| 162 | cancel_all | — | bullet manager 0x430830(4) | 10 |
| 163 | g_f54cec | i n | global 0xF54CEC | 0 |
| 164 | spell_mode | i n, f a, f b, f c | spell flag bit 6 := n & 1 (`Spell_SetFlag6` 0x41F0B0); n == 0: 0x41F040(a, b, c) | 0 |
| 165 | anm_rot_z | f a | main VM rotation.z = a | 28 |
| 166 | polar_ys | W x, W y, f ang, f r | y = sin(ang)·r, x = cos(ang)·r (no wrap) | 0 |
| 168 | item_burst1 | i n | n items of type 1 at random offsets | 22 |
| 169 | rand_away_angle | W a | if (pos.x > player.x and pos.x > 96) or pos.x > 288: a = wrap(R·π/2 + 3π/4) (leftward cone) else a = R·π/2 − π/4 (rightward cone); one Rng_Float | 0 |
| 170–172 | laser fields | i k, … | laser +0x599 byte / +0x560 / +0x558,+0x55C | 0 |
| 173 | flagA30 | i b | bit 30 | 118 |
| 174 | aura | i n | (re)create aura effect 0x20+n at final pos | 38 |
| 175 | spawn_disable | i n | global 0xF54E2C (timeline spawns suppressed if ≠ 0) | 5 |
| 176 | last_spell | (ignored arg) | GameFlags (0x164D0B4): clear bits 7, 8, 13, set bit 7; also bit 13 when (not spell practice and stage idx 6/7) or (practice and card id 0x8F–0x92 or 0xAB–0xBE); sets flag A bit 30. `Spell_Start` copies bit 7 into spell flag bit 5 (answers_impl10 §7, VERIFIED). While bit 13 is set and boss 0 exists, `EnemyMgr_Calc` calls `Player_DamageAgainst` with the whole playfield box (centre (192, 224), size (384, 448), accumulator boss+0x2E10) and discards the damage: all shots count as hits (answers_impl12 §5) | 41 |
| 177 | set_hud_life | i n | e+0x2E04 | 6 |
| 178 | move_rand2 | i dur, i ease, f spd | like 67, with 1/4 chance of a fully random angle (§8.3) | 4 |
| 179 / 180 | hud179 / hud180 | — | HUD functions (later phase) | 2 / 0 |
| 181 | extend181 | — | if game counter < 12: sfx 0x2D, counter += 1, HUD update (later phase) | 8 |
| 182 / 183 | flagB8 / flagA31 | i b | bit set | 30 / 61 |
| 184 | spell_ex | i n | spell flag bit 11 (bonus frozen) := n & 1 (`Spell_SetBonusFrozen` 0x41F0E0, §11.1) | 82 |

Any other opcode value is ignored (ip advances).

## 8. Movement — VERIFIED (`Enemy_UpdateMoveMode` 0x422C40, `Enemy_Move` 0x42DEB0, `Enemy_ClampPos` 0x42C180)

After the scripts each frame (gs = game speed):

| mode (bits 12–13) | per-frame update |
|---|---|
| 0 | nothing (velocity unchanged) |
| 1 polar | angle = wrap(angle + gs·angvel); speed += gs·accel; velocity = (cos angle·speed, sin angle·speed, 0); if duration > 0: timer.Decrement(); when timer ≤ 0 → mode 0 (velocity kept) |
| 2 interpolate | timer.Decrement(); t = max(0, 1 − timer/dur); t = ease(bits 14–16; same table as §5.6); velocity = (start + delta·t) − pos (x negated if mirrored); angle = atan2(vel); when timer ≤ 0: mode 0, pos = start + delta exactly, velocity 0 |
| 3 orbit | orbit angle = wrap(+ gs·orbit av); radius += gs·rv; velocity.xy = centre + (cos·r, sin·r) − pos; angle = atan2(vel); duration as mode 1 |

Then in the enemy update (unless flag A bit 29): clamp; `displacement = pos − prev; prev = pos;
pos.x += gs·vel.x (−= if mirrored); pos.y += gs·vel.y; pos.z += gs·vel.z`; clamp again; followers take the
parent's position as offset; final pos = pos + offset with z = 0.
Clamp (bit 19): x into [xmin, xmax], y into [ymin, ymax].

### 8.3 Random direction (op 67 / 178) — VERIFIED (0x422020 / 0x4224A0)
R = Rng_Float (each line one call). Clamp rect = e+0x3340 (xmin), +0x3344 (ymin), +0x3348 (xmax), +0x334C (ymax).
Op 67: a = R·(π/2) − π/4 if pos.x ≤ player.x, else wrap(R·(π/2) + 3π/4). Then, in order:
- pos.x < xmin + 96: a > π/2 → a = π − a; a < −π/2 → a = −π − a.
- pos.x > xmax − 96: 0 ≤ a < π/2 → a = π − (enemy's current angle e+0x2D94) (**QUIRK**: uses the old angle,
  not a); −π/2 < a < 0 → a = −π − a.
- pos.y < ymin + 48 and a < 0 → a = −a; pos.y > ymax − 48 and a > 0 → a = −a.
Op 178: with Rng_Range(4) == 0: a = Rng_SignedRange(π) (= R·2π − π). Otherwise (dx = player.x):
pos.x ≤ dx: if pos.x − (dx − 384) ≤ dx − pos.x → a = wrap(R·π/2 + 3π/4) else a = R·π/2 − π/4;
pos.x > dx: if (dx + 384) − pos.x ≤ pos.x − dx → a = wrap(R·π/2 − π/4) else a = wrap(R·π/2 + 3π/4);
then only the two y-edge flips of op 67.
Both: dur < 1 → mode 1 (angle a, speed arg, duration 0, timer 0). Else mode 2 (0x4222B0): delta = (cos a·spd·dur,
sin a·spd·dur, 0) (not mirrored), start = final pos, timer = dur, ease bits = arg.

## 9. Bullets and lasers

### 9.1 Bullet block (ops 96–104, 44-byte instruction) — VERIFIED (`Ecl_FireBullets` 0x422720)

| offset | type | mask bit | field |
|---|---|---|---|
| +0x0C | i16 | 0 | bullet type (sprite group in `etama.anm`) |
| +0x0E | i16 | 1 | colour / sprite offset |
| +0x10 | i32→i16 | 2 | count1 (bullets per row / ring) |
| +0x14 | i32→i16 | 3 | count2 (rows) |
| +0x18 | f32 | 4 | speed1 |
| +0x1C | f32 | 5 | speed2 |
| +0x20 | f32 | 6 | angle1 |
| +0x24 | f32 | 7 | angle2 |
| +0x28 | u32 | — | flags (0x200 play sfx; 2/4/8 spawn-effect styles; 0x8000/0x10000 mode filters; others: 08 spec) |

Firing (only if life > 0): if flag A bit 17 is set the instruction is stored for auto-fire instead. Skipped
if flags & 0x8000 and player not in youkai mode, or flags & 0x10000 and in youkai mode, or if no-fire radius²
> 0 and squared distance (final pos ↔ player) < radius². Origin = final pos + shot offset; aim mode = op − 96.

### 9.2 Rank adjustment (unless a spell card is active)
`lerp_i(a, b) = a + trunc((b − a)·rank / 32)`, `lerp_f(a, b) = a + (b − a)·rank / 32.0`, rank = var 10041 (actual range 8–16 by difficulty, so the lerp only covers the middle of [a, b]; integer division truncates toward 0)
(var 10041). count1 += lerp_i(c1a, c1b) (min 1); count2 += lerp_i(c2a, c2b) (min 1); if speed1 ≠ 0:
speed1 += lerp_f(s0, s1) (min 0.3); speed2 += lerp_f(s0, s1)/2 (min 0.3).

### 9.3 Pattern placement (`BulletMgr_SpawnPattern` 0x430E10 / `PlaceBullet` 0x42F5F0)
If live bullets < 1536: `aim = angle(origin → player)`; for j in 0..count2−1, i in 0..count1−1 (stop when no
free bullet): speed = (count2 < 2) ? speed1 : speed1 − (speed1 − speed2)·j/count2.

| op | mode | angle of bullet (i, j) |
|---|---|---|
| 96 | 0 fan aimed | k = count1 even ? a2/2 + (i/2)·a2 : ((i+1)/2)·a2 (integer i/2); if i odd k = −k; angle = k + aim + a1 |
| 97 | 1 fan | same without aim |
| 98 | 2 ring aimed | aim + j·a2 + a1 + i·2π/count1 |
| 99 | 3 ring | j·a2 + a1 + i·2π/count1 |
| 100 | 4 ring offset aimed | aim + i·2π/count1 + π/count1 + a1 |
| 101 | 5 ring offset | i·2π/count1 + π/count1 + a1 |
| 102 | 6 random angle | Rng_Float·(a1 − a2) + a2 |
| 103 | 7 random speed | speed = Rng_Float·(s1 − s2) + s2; angle as mode 3 |
| 104 | 8 random both | angle as 6 (RNG first), then speed as 7 |

Bullet start: position (origin.x, origin.y, 0.1), angle wrapped, velocity = polar(angle, speed·gs). After the
pattern, flags & 0x200 plays desc sfx at the origin. (Bullet state machine: 08 spec.)

Auto-fire (`Enemy_AutoFireAndDirAnim` 0x423150, only while life > 0): if interval > 0: timer.Tick(); when
timer ≥ interval: fire the stored instruction, timer = 0.
Direction animation (when the "left" script ≥ 0): dir = vel.x < −0.01 → 1 (left), > 0.01 → 2 (right), else 0
(swapped when mirrored). On change: to 1/2 start left/right script; back to 0 start idle (if previous −1),
left→idle or right→idle script.

### 9.4 Laser block (ops 114/115, 64-byte instruction) — VERIFIED field mapping
+0x0C i16 type, +0x0E i16 colour (mask bit 1), +0x10 angle (bit 2), +0x14 speed (bit 3), +0x18–0x24 four floats
(bits 4–7 → laser desc +0x1D0..+0x1DC: lengths/widths, 08 spec), +0x28–0x30 three ints (bits 8–10 →
+0x1E0..+0x1E8: timings), +0x34, +0x38 raw → +0x1EC, +0x1F0, +0x3C raw flags → +0x1FC; desc +0x1F8 =
1 for op 114, 0 for op 115. Origin = final pos + shot offset. Result stored in laser slot[current].

## 10. Enemy update, damage, death — VERIFIED (`EnemyMgr_Calc` 0x42C660)

### 10.1 Per frame, in order
1. Timelines (§3), then for each enemy slot in index order (alive only):
2. If flag A bit 10 (being chain-killed): only position follow + death processing. Else:
3. If bit 8: react to player mode changes (effects, aura interrupt, mode bits e+0x3330 = 0x20/0x40, bit 11).
4. Repeat { `Ecl_Run` (−1 → free enemy, no drops) ; movement (§8) ; screen check ; } while
   `Enemy_CheckLifeThresholds` or `Enemy_CheckTimeout` request a re-run (§10.3).
   Screen check (unless bit 4 or already done): when the box (final pos ± hitbox/2) first overlaps
   [0,384]×[0,448], set bit 24; afterwards, if it no longer overlaps and bit 28 is clear → free enemy.
5. Run main ANM VM and the two sub VMs (a finished sub VM is disabled).
6. Unless bits 4/5/11 (and the player-state exception): contact test with the player (bit 2, 0x42C290, box
   = hitbox e+0x2D70): (a) if flag A bit 7 and the enemy timer e+0x2E14 advanced and is a multiple of 6:
   `Player_GrazeTest` with box = hitbox / 0.7 (larger; grazes the player every 6 frames); (b) unless shot type
   0/4 and the enemy is a chained child: `Player_HitTestNoBomb` 0x44A360 with box = hitbox / 1.5 (smaller):
   kills the player in state 0; result 1 (also for an invulnerable player) on a non-boss without bit 7 →
   enemy life −= 10 (every frame of contact). Then, if bit 6, shot damage `d` = `Player_DamageAgainst`(hitbox,
   …, &hitByBomb) where hitByBomb starts as "player is bombing"; if the extra shot box (e+0x2D7C) has x > 0, a
   second call d2 against it, and when hitByBomb is still 0: d = trunc(d + d2/6.5) for shot types 3/11, else
   trunc(d + d2/1.7). If d > 0:
   - `d = min(d, 70)` (values ≥ 70 become 70); score += (d / 5)·10 (integer division);
   - if bit 3: while a spell card is active: normal damage `d < 8 ? min(d,1) : d/7`; bomb damage (hitByBomb) =
     0 unless spell flag bit 7 is set ("bomb started during this card": set by `Player_UpdateBomb` → 0x44CBA0, which
     also clears the capture bit 2 and the card bonus; cleared at card start — so a bomb started before the card
     deals 0) and the enemy is not a chained child, then
     `d < 3 ? min(d,1) : trunc(d/2.5)` (VERIFIED 0x42D2B9); if the
     damage-reduction timer (op 160) > 0: bosses `d/9`, others 0; life −= d; var 10083 = d; chained enemies
     forward damage to their root (0x42B370: root life −= d/2, or /9 / 0 under the root's timer, never below
     the root's highest active threshold).
7. Hit flash: on a hit the main VM uses colour2 = (R 0xFF, G 0x60, B 0x80, alpha = current) for one frame
   with sfx 0x14 (0x25 when the low-life warning level ≥ 2); in youkai mode the enemy is drawn with colour2
   (0x20, 0x20, 0xC0, alpha/2) permanently. Then boss HUD life bar, effect followers, timer 0x2E14 tick
   (unless global 0x160F534), damage-reduction timer decrement, add to draw list (layer e+0x332F) unless bit 4.

### 10.2 Life thresholds (op 133) and timeout (op 134)
- Thresholds (checked after every Ecl_Run): for each active k: if life < threshold: life = threshold;
  main context jumps (no push, depth 0) to its sub; threshold cleared; bonus seconds
  (timeout − timer)/60 stored; timeout cleared; threads freed; bullet descriptor reset; auto-fire off;
  rank ranges reset (speed ±0.5, counts 0); children chain-killed (with drops); every alive non-boss
  enemy gets life 0 and jumps to its death sub; boss: global player-side timer set (70). The script is
  re-run in the same frame.
- Timeout: when timer (e+0x2E14) ≥ timeout: life = largest active threshold (cleared); jump to the timeout
  sub; timeout sub := death sub; timer = 0; unless bit 27: spell card ends and bullets are cancelled; same
  cleanup (children killed **without** drops). Re-run in the same frame.

### 10.3 Death (life < 1, flags B bits 3 and 6 clear)
flags B bit 3 set; chained children killed with drops; a game gauge changes by ±200 (player-mode
dependent, later phase); then by death mode (op 129):

| mode | behaviour |
|---|---|
| 0 | add score; enemy dead (alive cleared); aura effect told to end |
| 1 | add score; bit 23; contact/damage/shot bits cleared (stays alive, e.g. scripted death animation) |
| 2 | no score; stays alive |
| 3 | life = 1, death latch/mode cleared, 3 death effects, aura ends (scripted "survive") |
| 0–2 | if boss: HUD off and boss unregistered; item drops (§10.4); a boss without an active spell card also turns all bullets into bonus and kills other enemies (score + HUD) |

Unless bit 10: sfx 2/3, death effects (bytes e+0x3310/0x3311). Then, if a death sub is set: context reset
(depth 0, threads freed, descriptor reset) and the main context jumps to the death sub (the enemy keeps
running its script), death sub := −1.

### 10.4 Drops (`Enemy_DropItems` 0x42BEA0)
item ≥ 0: spawn item `item` at final pos (+ effect). item == −1: on every third such death (global counter
% 3 == 0) spawn item from the 32-entry byte table 0x4C70D8 (cyclic index):
`00 00 01 00 01 00 00 00 01 01 00 00 01 01 01 00 01 00 01 00 01 00 01 00 01 00 00 01 01 01 00 00`.
item ≤ −2: nothing. Then e+0x330C power items (type 0 if player power < 128 else 1) and e+0x3308 type-1 items
at final pos + (Rng·128 − 64, Rng·128 − 64); counts reset. (Item types: 08 spec.)

### 10.5 Child chains (ops 90–92)
Spawn fails (manager flag) if the parent's life < 1 or it is being chain-killed. The new enemy (vars copied,
§4.1) is appended to the parent's chain (root = parent, linked via +4/+8), parent child count += 1; it gets
bit 8 (reacts to player mode), bit 11 = current mode, layer 0/2, contact damage off, an aura effect (0x20)
interrupted with 1/2 by mode. Op 91 position relative to the parent's final pos; op 92 additionally makes the
child follow the parent (offset = parent pos, bit 9). Sound 0x24. When the parent dies or passes a threshold,
children are killed (bit 10) with score popups / item showers proportional to chain length (0x42ADB0).

### 10.6 Enemy drawing — VERIFIED (0x42E140; draw prio 8 → layers 0–1, prio 11 → layers 2–3)
Enemies are linked each calc into 4 lists by the layer byte e+0x332F (template 0; op 159 = value, op 156 = 2,
mode-reacting enemies 0 in youkai / 2 in human mode). Prio 11 draws with the global tint 0xFFF01010 while
bullets are frozen (0x164D0B4 bit 10). Per enemy, all with Draw2DRot; positions + playfield origin (0x164D2DC = 32, 16):
1. sub VM 0 (e+0x2B0, if its script ≥ 0): rotation = enemy angle (e+0x2D94) if the VM tag ≠ 0; pos = final pos
   + (flags B bit 8 ? main VM pos offset : its own pos offset), z 0.3;
2. main VM (e+0xC): rotation = angle if flag A bit 25; pos = final pos + main VM pos offset (+0x288), z 0.25;
   optional after-image trail (e+0x534C flags, ring buffer at e+0x3394…, op 157) drawn first;
   skipped when flag A bit 5 (invisible) or trail flag 0x10;
3. sub VM 1 (e+0x554): as sub 0 but rotation = −angle.
Death subs: e+0x2CEE is consumed (set −1) by every jump: normal death (with full context reset), `EnemyMgr_KillAll`
0x42EFB0, and `Enemy_CheckLifeThresholds` 0x42B490 / `Enemy_CheckTimeout` 0x42B930 — the latter two set life (e+0x2DFC) := 0 for
**every alive non-boss enemy** and run its death sub, when a boss passes a threshold/timeout (main context ip/clock/wait
reset only).

## 11. Spell card (op 122, 244 bytes) — VERIFIED

| off | type | meaning |
|---|---|---|
| +0x0C | i16 | portrait/face parameter |
| +0x0E | i16 | spell id |
| +0x10 | i32 | bonus |
| +0x14 | 48 B | name, each byte XOR 0xAA (Shift-JIS) |
| +0x44 | 48 B | owner name, XOR 0xBB |
| +0x74 | 64 B | comment line 1, XOR 0xDD |
| +0xB4 | 64 B | comment line 2, XOR 0xEE |

Effect: spell-card state on; bonus decays every second by (bonus − bonus/7) / (timeout/60) (timeout from op 134,
which precedes it in all data); flag A bit 27 forces bonus 99 999 990; bullets cancelled; stage spell
background started; rank ranges reset; score.dat history updated (later phases). The disassembler prints the
decoded strings.

### 11.1 Spell flags and the end of a card (`Spell_End` 0x4161B0, op 123) — VERIFIED
Spell object (g_Gui 0x4EA670) flag word: bit 0 active, bit 2 capture still possible (set at start; cleared by a
bomb started during the card 0x44CBA0, by the player's real death 0x44D150, by a timeout 0x42BC50), bit 3 timed
out (0x42BC50, from `Enemy_CheckTimeout`), bit 5 last spell (:= GameFlags bit 7 at `Spell_Start`, i.e. op 176 ran before the card; failing such a card shows message 6 "Last Spell Failed", answers_impl10 §7), bit 7 "bomb started during this card" (bomb damage
allowed, §10.1), bit 8 capture payout pending, bit 9 captured, bit 10 name-only declaration (set by native 22 `Spell_ShowNameOnly` 0x415D10, which also
clears bits 0 and 4 and shows the name banner; the bonus/history display is skipped; used by Mokou's
「リザレクション」 in ecldata8/8sp), bit 11 bonus frozen (op 184 `Spell_SetBonusFrozen` 0x41F0E0 = n & 1; no
decay in `Spell_Calc` 0x416B90 and time-orb +8000 in `Spell_AddBonus` 0x416B10 ignored; all 82 uses are
`spell_ex 1` at the start of boss death subs). Both cleared by `Spell_Start`; bit 11 also by `Spell_End`.
Bit 6 is set by op 164 (`Spell_SetFlag6` 0x41F0B0). Bonus +0xFC.
On op 123 with the card active: bit 0 off; if not timed out → `CancelBonus(8000, 1)` + `KillAll(8000)` →
score + `Hud_ShowBonus`; if capture still possible → captured: payout value = +0xFC, time reward 700 (flag A
bit 27) or from the remaining card time (1000 / 100 / linear, answers_phase5 impl8), captures counter, score-file
stats, checksum (RNG), start-ring recoloured, SE 0x23, bits 8 and 9; payout 130 frames later in the spell calc
0x416B90 (`Hud_ShowSpellBonus`, AddScore, AddTime, max point value += reward·10) or at the next card start.
Not captured → HUD message 5 / 6 (last spell). Always: SE 0xF; player in state 0 → state 3 for 70 frames; stage
spell background off.

## 12. Native functions (op 136 once, op 137 every frame) — VERIFIED behaviour summary

Called with the enemy and the instruction; `arg` = second operand (instr +0x10).
| n | behaviour |
|---|---|
| 0 | globals 0x4E3D28 = var 10000, 0x4E3D24 = var 10016 |
| 1, 17 | screen flash (type 3: duration 60 (native 1) / 180 (native 17), 1 repeat, 0xFFFFFFFF; prio 0x15; 13 §3) |
| 2 | bounce: vel.x negated at x ≤ 0 / ≥ 384; vel.y += var 10022 while vel.y < var 10023; vel.y negated when y < −64; clears bit 28 when y ≥ 480; angle = atan2(vel) when changed |
| 3, 6, 20 | create two effects (0x38 / 0x3A / 0x41) with an extra ANM script (0x61 / 0x65 / 99) and set the stage callback (0x4235A0) |
| 4, 7, 21 | bullet "warp" across rectangular zones (zone constants 124/260/140/276 & 56/328/72/344; 56/328/88/360 & −32/416/0/448; 112/271/128/287 & 33/350/49/366) with scaling about the centre; bullets reverse direction |
| 5 | end effects 9 and 10; global 0x4E4B60 = 2 |
| 8 | familiar ring: chain members with the same tag (var 10038) get an index (var 10037) and count (var 10018, change flag var 10017); each turns its orbit angle toward leader + index·2π/count by 2 % of the difference |
| 9, 11, 25 | wide horizontal player-hit rectangles (590 × 160/128, 240/192, 288/224) at the boss with a 12-frame warning phase |
| 10 | flash (type 3: duration 30, 5 repeats, 0x40FFFFFF) + shake (type 7: amplitude 4, ramp 120, hold 190, out 60) (13 §3) |
| 12, 14 | toggle bullets whose flags match var 10000 between two states (sprite ±16, speed from var 10016/10017); 12 also walks the calling enemy's child chain (e+8 = next: parent+8 → first child → child+8 …, e+4 = prev): var 10001 == 0 → set flags B bit 7 on every chained child and send interrupt 2 to the two spell-background VMs (0x4E4B68 / 0x4E4E0C); else clear bit 7 and send interrupt 1 (answers_impl10 §4, VERIFIED) |
| 13 | stage tint colour 0xFFC03030 |
| 15 | shake (type 7: amplitude 16, ramp 20, hold 20, out 20) (13 §3) |
| 16 | bullets with flag 0x100000 within 64 px of a child whose var 10038 == 0 set its var 10038 = 60 and copy var 10019 |
| 18 | game speed = 1/arg |
| 19 | var 10000 = current spell id |
| 22 / 23 | 22: name-only spell declaration (spell flag bit 10, name banner of the string at 0x4B495C 「リザレクション」, §11.1); 23: hide the name banner (0x416160) |
| 24 | var 10000 = game value at (0x160F510)+0x1C |
| 26 | global 0x160F534 = arg (pauses enemy timers; stage VMs interrupt 2, or 1 when 0) |
| 27 | each bullet with flag 0x100000 spawns enemy sub = var 10038 at its position (life 800, item −2, score 10, var 10016 = bullet angle); flag cleared |
| 28 | slow-down: game speed = 1/arg; bullets' velocities scaled; bullet sprites 0x60–0x6F → 0x6F; stage VMs interrupt 2 |
| 29 | undo 28: speed 1, velocities restored, sprites restored, interrupt 1 (Supervisor flag 0x20 if arg > 1) |
| 30 | global 0x18B8A2C = arg |
| 31 | item at pos: type 5 (type 3 when global 0x17D6ED4 ≠ 0) |

## 13. Verification

`python3 -I tools/ecldis.py extracted/dat -q` → coverage of sub ops 0–184 present in data, timeline ops,
"unknown ops: [] []", "problems: 0" (sizes vs. signatures, jump targets, terminators, sub ids). Spell names
decode to valid Shift-JIS text (e.g. stage 1 boss spells), confirming the XOR keys.

## 14. Open points
- Exact semantics of the bullet extra commands (op 111), laser fields and item types belong to 08.
- Player-side functions used here (damage computation, hit rectangles, mode byte 0x17D5EF8+5) → 09.
- HUD/dialogue functions (timeline 6/7, ops 148/158/179–181) → 10/11.
- Flag A bits 7, 23, 25, 30, 31: set by ops but consumers only partly traced.
