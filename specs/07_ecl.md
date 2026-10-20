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
Only pos (+0x2D34) is written by the spawn; previous pos +0x2D58, displacement +0x2D64, position offset +0x2D40 and final pos +0x2D88 keep the template's values, which are all (0, 0, 0) (`EnemyMgr_Ctor` zeroes the template). `Ecl_Run` recomputes final pos = pos + offset before each instruction, so the spawn run already sees the spawn position there. The first `Enemy_Move` (§10.1 step 4) computes displacement = pos − (0, 0, 0) = the spawn pos, so vars 10085–10087 read the spawn position until the second move (QUIRK; harmless in the shipped data: the 4 reads are at sub time 60) (VERIFIED 0x42A4E0, 0x42DEB0, 0x429E00, RE-Q24).
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
| 0x2CE8 / 0x2CEA | i16 | main context's stack depth (persistent) / working depth of the context being run (§5.4: `Ecl_Run` reloads 0x2CEA from 0x2CE8 at entry; changelog/answers_boss_phases.md §3) |
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
| 0x2E10 | i32 | time-orb damage accumulator (passed to `Player_DamageAgainst`, 09 §4.5). Starts at the **step** (40, 27 solo human): `Player_Load` writes the template's copy (0x57AD30 = template+0x2E10) and `EnemyMgr_Ctor` copies player+0xE2B2C into it; every spawn copies the template, so a fresh enemy's first hitting bullet spawns an orb at human max gauge. Per enemy, never shared, never reset (VERIFIED runtime, changelog/answers_impl16.md §2) |
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
| 0x3380 | i32 | chained children created (ops 90–92 +1) minus children killed by damage while chained (`EnemyMgr_Calc` −1); never reset by a chain kill (changelog/answers_impl12.md §2) |
| 0x3384 | ptr[4] | extra script threads (op 135) |
| 0x3394 | 96 × 0x1C | after-image trail ring (op 157): per entry final pos[3], velocity[3] (e+0x2D4C), angle (e+0x2D94); template x = −999 (changelog/answers_impl21.md §Q7) |
| 0x534C / 0x534E / 0x5350 / 0x5352 | u8 / i16 ×3 | trail flags (≠ 0 = trail on; 8 = allocate the draw history, 0x10 = main VM hidden) / length n / op 157 arg a / draw step b |
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
0x17D5EF8 ≠ 0; changelog/answers_impl10.md §4) · 8 (op 182).

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
  thread slot (context +0x220) − 1, switch back to the main context with depth := e+0x2CE8, continue with the next
  context (for the main context itself this would address slot −1 = e+0x3380, the child counter: malformed, never in
  the data — do not emit); else restore the whole context from stack[depth] (ip, clock, local
  vars …) and continue.
- **thread (op 135 slot, sub)**: free thread slot; if sub ≥ 0 create it (copy of the current context's 30
  var dwords, clock 0).
- Op 88 calls a sub *in another enemy's* (boss slot) main context the same way.
- **Depth storage** (VERIFIED code + runtime, changelog/answers_boss_phases.md §3): the main context's depth lives in e+0x2CE8; e+0x2CEA is
  only the working depth. `Ecl_Run` starts with 0x2CEA := 0x2CE8, swaps in each thread's own depth (thread +6) while
  running it, and writes 0x2CEA back to 0x2CE8 (main) or thread +6 after each context. The "depth := 0" written by the
  life-threshold, timeout and death-sub handlers (§10.2/§10.3) goes to **0x2CEA only**, so it is overwritten at the next
  `Ecl_Run` entry: a phase change does **not** reset the main stack depth. The new phase sub runs at the depth the old
  phase had at that moment (e.g. 1 after interrupt entry, 2 if the change happened inside a called sub), and its calls
  push from there. Stale frames below are never popped in the data (no threshold/timeout/death/interrupt target sub
  executes `return`; checked over all 305 targets), but the 15-push cap (§5.4 call) is reached earlier.

### 5.5 Interrupts — VERIFIED
`e.pending_interrupt = k` (set by op 125 immediately, op 89 / timeline op 8 on another enemy). At the next
LOOP check: ctx.ip = ip + size of the **current** (not yet executed) instruction — i.e. that instruction is
skipped when the interrupt returns; unless flag A bit 26: push the context; set sub = interrupt_table[k]
(e+0x2CF0, op 126); clock = wait = 0; depth += 1 (≤ 15) — **also when flag A bit 26 suppressed the push** (calls
only increment when pushing); pending = −1. Interrupts always target the **main** context (e+0x7F8) and do **not**
copy the 8 argument dwords (only calls do). VERIFIED (changelog/answers_phase5.md impl5).
**Thread contexts** (VERIFIED code 0x41C88A, RE-Q24): the pending test sits at the LOOP of **every** context, threads
included. Taken while a thread is the current context, it mixes the two: the **thread's** ip skips its current
instruction; the **main** context is pushed onto the **thread's** call stack at the thread's depth (unless bit 26);
the main context gets the interrupt sub (clock, wait 0); the thread's depth += 1 (main depth e+0x2CE8 unchanged);
pending = −1; then the **thread** keeps executing this frame and the main context starts the interrupt sub next
frame. Unreachable with the shipped data: pending is set only by op 125 (0 uses), op 89 (0 uses) and timeline op 8
(runs before all enemies, §10.1 step 1, so the main context, visited first, always takes it).

### 5.6 Variable interpolation slots (op 36) — VERIFIED
Slot = {fn, Timer elapsed, dur, fn index, easing mode, a, b, c, d, target var id}. Op 36 reuses the slot whose
target equals the new target, else the first free slot (8 max; ignored when full). Each POST:
`elapsed.Tick(); if elapsed ≥ dur: elapsed = dur; t = elapsed/dur; t = ease(mode, t)` (1 t², 2 t³, 3 t⁴,
4 1−(1−t)², 5 1−(1−t)³, 6 1−(1−t)⁴, else t); `fn(t)`; slot freed when elapsed ≥ dur.
fn index 0–6: `var = a + (b − a)·t`; 7: Hermite `var = (2t+1)(t−1)²·a + (3−2t)t²·b + t(1−t)²·c + (t−1)t²·d`.
**a–d are resolved once, by op 36**: an operand whose mask bit is set (bits 4–7 for a–d; bits 1–3 for dur, fn, mode, read as ints) is read through the variable getter at op time, and the resulting float is stored in the slot (bit copy). At every evaluation the fns pass the stored floats through the float getter again, but the getter returns its argument unchanged unless it truncates to a variable id; no shipped op 36 has a stored a–d in the variable range, so in practice later writes to the source variables (e.g. 10018/10019 changed by op 27/op 9 in another slot's thread) do not affect a running interpolation (VERIFIED 0x4213F0, 0x421120, 0x421180, getter default 0x420120; RE-Q24; corrects the earlier "re-read at every evaluation"). If any slot targets
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
| 10096 | chain count (`Enemy_ChainChildCount` 0x41FD40): if the enemy is a chain root (no parent +0x2DA4 and +8 ≠ 0) = number of enemies reached by following +8 from it; else if it has a parent = the same count evaluated on the **parent** (+0x2DA4, the enemy that spawned it with op 90–92), which is 0 unless that parent is a root; else 0. No alive test: every death, `Enemy_Free`, kill-all and chain kill unlinks the enemy (`Enemy_ChainUnlink`), so the walk only meets live members. VERIFIED code + demo 3 runtime (changelog/answers_impl19.md §Q3, changelog/answers_impl21.md §Q1) | — | |
| 10097 | player mode byte (0x17D5EF8+5) | — | |
| 10098 | (int only) **2 if a + b + c ≥ d, else 0** — a = stage time (stats +0x3C), b = pending time reward of a captured card not yet paid (spell object g_Gui +0x10), c = number of time-orb items (type 7) currently in the item list, d = stage time threshold (stats +0x40 = `TIME_THR`[stage][difficulty], 11 §3.1). Read by the Last Spell host sub of each stage boss (ecldata1–5: `≠ 2 → delete`). VERIFIED code + runtime (changelog/answers_impl20.md §Q1) | — | |
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
| 36 | interp | target var id stored as a **float** (e.g. 10016.0; the raw dword is kept in the slot and compared as a float, all 37 uses), i dur, i fn, i mode, f a, b, c, d | start interpolation slot (§5.6) | 97 |
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
| 66 | move_dir | i dur, i ease, f ang, f spd | dur < 1: as 65. Else mode2 with delta = (cos a·spd·dur, sin a·spd·dur, 0), start = final pos; **delta.x negated if mirrored** (bit 18; `Ecl_OpMoveDir` 0x420D10, VERIFIED changelog/answers_impl16.md §8) | 188 |
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
| 95 | kill_all | — | every alive non-boss enemy without flags B bit 6: life = 0 (dies in its own update), jumps to its death sub if any; enemies with flag A bit 7 also drop item type 6 and a score popup (2000, +30 per enemy, capped at 8000) (0x42EFB0). **No self test**: the calling enemy is included unless it is a boss or has flags B bit 6 (same for the `KillAll` of op 123 / death modes 0–2) (VERIFIED, RE-Q24) | 157 |
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
| 148 | hud148 | i n | HUD 0x423130(n): remaining-bars count g_Hud+0x20 = n (12 §5.1, §5.4); global 0x164D30C += 1800 | 188 |
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
| 176 | last_spell | (ignored arg) | GameFlags (0x164D0B4): clear bits 7, 8, 13, set bit 7; also bit 13 when (not spell practice and stage idx 6/7) or (practice and card id 0x8F–0x92 or 0xAB–0xBE); sets flag A bit 30. `Spell_Start` copies bit 7 into spell flag bit 5 (changelog/answers_impl10.md §7, VERIFIED). While bit 13 is set and boss 0 exists, `EnemyMgr_Calc` calls `Player_DamageAgainst` with the whole playfield box (centre (192, 224), size (384, 448), accumulator boss+0x2E10) and discards the damage: all shots count as hits (changelog/answers_impl12.md §5) | 41 |
| 177 | set_hud_life | i n | e+0x2E04 | 6 |
| 178 | move_rand2 | i dur, i ease, f spd | like 67, with 1/4 chance of a fully random angle (§8.3) | 4 |
| 179 / 180 | night_clock_show / night_clock_hide | — | 179 (0x439007): HUD night-clock VM impl+0x2156C starts `times.anm` script 2 with sprite = clock counter; 180 (0x4390D6): that VM's alpha byte := 0 (12 §5.7) (VERIFIED, RE-Q25) | 2 / 0 |
| 181 | extend181 | — | only if the night clock stats+0x28 < 12 (else nothing at all, no sfx): `Sound_Play`(0x2D), clock += 1, then the HUD night-clock VM (impl+0x2156C, times.anm, 12 §5) gets sprite = clock and interrupt 2 if the clock is now 12, else interrupt 1 (VERIFIED, RE-Q24) | 8 |
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
- pos.y < ymin + 48 and a < 0 → a = −a; **then, as a separate test on the updated a**, pos.y > ymax − 48 and a > 0 → a = −a.
  When the band overlaps (ymax − ymin < 96, e.g. the stage-1 boss's clamp rect 48..128: 80 < y < 96) both can fire in
  one call: a < 0 → +a → −a again, so the move always goes up (a ≤ 0). Example: fullrun_noclip row 6251, boss at
  y 86.71 moves up (VERIFIED 0x422020 and runtime scan, RE-Q24). The x tests are likewise two separate `if`s. All tests
  use the base pos e+0x2D34, not the final pos.
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
3. If bit 8: `Enemy_ReactPlayerMode` 0x42C420 (changelog/answers_impl14.md §1, VERIFIED at runtime). Player mode = player
   byte +5 (0x17D5EFD, `FUN_0040bc40`). Using the stored mode bit 11 **at entry**:
   - bit 11 clear (enemy in human state): if mode ≠ 0 → effect 0x1F at final pos (count 1, colour 0x80303080),
     aura (e+0x53C8) interrupt 2, sfx 0x28, layer := 0. Then (still this branch, also on the switching frame):
     if flags B bit 1 (op 83) and timer e+0x2E14 "changed and even" (current ≠ previous and current % 2 == 0,
     `FUN_0040ebc0(t, 2)`) → `Effect_Spawn(38, final pos e+0x2D88, 1, 0xFFFFFFFF)`. Effect 38 = etama script
     71, which runs at spawn: `frand` (1 Rng_NextFloat = 2 Next16) then 4 × `irand` (4 Rng_Range = 8 Next16) =
     **10 draws**, in that order. The timer is 0 with previous −999 on a fresh enemy (template Timer_Set), so
     the first draw happens on the enemy's first update, then every 2nd frame (timer ticks at step 7).
   - bit 11 set (youkai state): if mode == 0 → effect 0x1E (0x80803030), aura interrupt 1, sfx 0x27, layer := 2.
     No effect 38 in this state.
   - Finally bit 11 := mode & 1; mode bits e+0x3330 := 0x40 if mode ≠ 0 else 0x20.
4. Repeat { `Ecl_Run` (−1 → free enemy, no drops) ; movement (§8) ; screen check ; } while
   `Enemy_CheckLifeThresholds` returns 1, or (timeout ≥ 0 and `Enemy_CheckTimeout` returns 1) (§10.2). On a phase-change
   frame the whole body runs twice: the old phase's instructions for this frame run first, then the new sub's time-0
   instructions, and the clamp/move/clamp step, the end-of-`Ecl_Run` move-mode update and auto-fire run again
   (changelog/answers_boss_phases.md §2).
   Screen check (changelog/answers_impl18.md §IMPL19-Q2, VERIFIED code + runtime): if the main VM has no sprite (e+0x230 = 0)
   bit 4 is set first (the enemy is then never screen-checked, linked or hit). The box is the **main VM's current
   sprite** size (sprite +0x34 width, +0x30 height, unscaled), centred on the final pos — **not** the hitbox. "On
   screen" (`GameMgr_IsOnScreen` 0x4399AC) = x + w/2 ≥ 0 and x − w/2 ≤ 384 and y + h/2 ≥ 0 and y − h/2 ≤ 448
   (inclusive). Unless bit 4: while bit 24 is clear, the first on-screen frame sets bit 24 (no free test on that
   pass); once bit 24 is set and bit 28 is clear, the enemy is freed (`Enemy_Free`, no drops) when it is off
   screen — for an enemy with an after-image trail (e+0x534C ≠ 0) only when both the current pos and the
    oldest trail pos (entry n − 1) are off screen (same sprite box).
   Trail ring (VERIFIED code, changelog/answers_impl21.md §Q7): 96 entries × 0x1C at e+0x3394 = {final pos x, y, z; velocity
   x, y, z; angle}. The template sets every entry's x to −999 (others 0) and every spawn copies the template, so
   unwritten entries are off screen. On every pass of this loop, after movement and before the screen check, while
   e+0x534C ≠ 0: entries shift up (for i = n − 1 down to 1: entry[i] = entry[i − 1], n = e+0x534E), then
   entry[0] = current final pos, velocity, angle. A phase-change re-run therefore shifts twice that frame. Data: n 15
   (21 uses) and 90 (1 use).
5. Run main ANM VM and the two sub VMs (a finished sub VM is disabled).
6. Unless bits 4/5/11 (and the player-state exception): contact test with the player (bit 2, 0x42C290, box
   = hitbox e+0x2D70): (a) if flag A bit 7 and the enemy timer e+0x2E14 advanced and is a multiple of 6:
   `Player_GrazeTest` with box = hitbox / 0.7 (larger; grazes the player every 6 frames); (b) unless shot type
   0/4 and the enemy is a chained child: `Player_HitTestNoBomb` 0x44A360 with box = hitbox / 1.5 (smaller):
   kills the player in state 0; result 1 (also for an invulnerable player) on a non-boss without bit 7 →
   enemy life −= 10 (every frame of contact). Then, if bit 6, shot damage `d` = `Player_DamageAgainst`(hitbox,
   …, &hitByBomb). hitByBomb is initialised to "player is bombing" (player +0xFDC, set for the whole bomb), but
   `Player_DamageAgainst` overwrites it: when the player's state timer (+0xE2AF4) has advanced (current ≠
   previous, i.e. every normal frame) it writes 0 at entry and 1 only when a damage area of the first 192
   (+0xB8834) **overlaps the enemy box on this call while life % period == 0** and +0xFDC ≠ 0 (changelog/answers_impl18.md
   §Q2, VERIFIED code + runtime). So a kill by shots during a bomb has hitByBomb 0 → drop mode 0. If the extra shot
   box (e+0x2D7C) has x > 0, a second call d2 against it (it overwrites hitByBomb again), and when hitByBomb is
   still 0: d = trunc(d + d2/6.5) for shot types 3/11, else
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
7. Hit flash (counter byte e+0x3314, changelog/answers_impl18.md §Q3, VERIFIED code): if the counter is 0 and the enemy took
   damage this frame (d > 0 at step 6) → colour2 = (R 0xFF, G 0x60, B 0x80, alpha = current), VM colour2 flag on,
   sfx 0x14 (0x25 when the low-life warning level ≥ 2), counter := 1; else if the counter ≠ 0 → counter −= 1 and
   flag off (no sfx even if hit); else flag off. Continuous hits therefore flash every 2nd frame; in youkai mode the enemy is drawn with colour2
   (0x20, 0x20, 0xC0, alpha/2) permanently. Then boss HUD life bar, effect followers, timer 0x2E14 tick
   (unless global 0x160F534), damage-reduction timer decrement, add to draw list (layer e+0x332F) if still alive
   (bit 0) and bit 4 clear: **prepend** (e+0 next := head; head := e). The 4 heads are cleared at the start of
   every `EnemyMgr_Calc` (before the timelines). Paths that skip the link: inactive slot, `Ecl_Run` −1 / screen
   free (`Enemy_Free`), death mode 0 (alive cleared). Timeline spawns happen before the slot loop, so they are
   processed (their timer reads 1 after the spawn frame) and linked in the spawn frame; an enemy spawned by
   another enemy's ECL (ops 90–92, child) into a higher slot is processed and linked the same frame, into a
   lower slot only from the next frame.
   Aimed enemy (14 §3.1): the aimed-pointer update is done in step 6 right after the damage, **before** the death
   check — an enemy killed this frame (even one spawned and killed in the same frame) can become the aimed enemy,
   and death mode 0 does not clear the pointer (only `Enemy_Free` and the next frame's inactive-slot test do).

### 10.2 Life thresholds (op 133) and timeout (op 134) — VERIFIED (code + runtime, changelog/answers_boss_phases.md)
Both checks run inside the step-4 loop, i.e. **after this frame's `Ecl_Run` and before this frame's damage (step 6)**:
damage that takes life below a threshold on frame N switches the phase on frame N+1.

`Enemy_CheckLifeThresholds` 0x42B490 (every pass):
1. Clear the low-life warning level (flags B bits 4–5).
2. For k = 0..3 in order, skipping thresholds < 0: if life < threshold[k] (strict): **switch** (below) and return 1 —
   the first matching index wins, later ones stay armed. Otherwise raise the warning level to
   `level(life − threshold[k])` if higher.
3. If no threshold is active, raise the level to `level(life)` with the no-threshold table. Return 0.
   Warning levels (3 / 2 / 1 / 0): with thresholds, diff < 500 / 1500 / 2200 (no card) or < 120 / 200 / 300 (card
   active); without thresholds, bosses life < 600 / 1600 / 2400 (no card) or < 120 / 300 / 400 (card), non-bosses only
   level 3 at life < 50 (no card) or < 10 (card). Used for the hit sfx (0x25 at level ≥ 2, §10.1 step 7) and the HUD
   enemy-marker blink (level + 1, else the hit-flash flag).

Switch (threshold k):
- life := threshold[k]; HUD life e+0x2E04 := life (max life e+0x2E00 is **not** changed, so the boss bar shows
  life/max of the previous `set_life`); main context ip := sub[k], clock := 0, wait := 0, sub id (`Ecl_SetSub`;
  variables, interpolation slots, interrupt table and the per-frame callback are kept); threshold[k] := −1;
- spell-capture bonus seconds e+0x53CC := (timeout − timer e+0x2E14)/60 (integer); timeout := −1 (the timeout **sub**
  and the **death sub are kept** — op 153 later copies the death sub);
- the 4 threads freed; rank ranges reset (speed −0.5…+0.5, counts 0); working depth 0x2CEA := 0 (no effect, §5.4);
  warning level cleared; bullet descriptor e+0x2E24 reset from the template (0x84 dwords); auto-fire interval
  e+0x3060 := 0; enemy timer e+0x2E14 **not** reset;
- `Enemy_KillChildren(e, with drops)`; every alive **non-boss** enemy (flags A bit 1 clear, any slot): life := 0 and, if
  it has a death sub, its main context jumps there (death sub := −1); its normal death runs in its own update;
  the loop runs over all 480 slots with **no self test**: the switching enemy is spared only by its own boss flag, so a
  non-boss that switches a threshold also gets life := 0 and jumps to its death sub (VERIFIED 0x42B490, RE-Q24);
- if e is a boss (flags A bit 1) and the player state byte (0x17D5EF8) is 0: player state := 3 with state timer
  (0x18B89EC) := 70 (the same invulnerable state `Spell_End` uses);
- no bullet cancel, no spell-card change, no HUD change here — those come from the new sub's own ops (`kill_all`,
  `spellcard`, …) in the immediate re-run.

`Enemy_CheckTimeout` 0x42B930 (only while timeout e+0x3378 ≥ 0, and only if the threshold check returned 0):
- every call on a boss with boss slot byte e+0x3313 == 0: HUD timer seconds (Hud +0x24) := (timeout − timer)/60;
- fires when timer ≥ timeout (`Timer_GreaterEq`; op 134 sets the timer to 0 in the same frame and it ticks at step 7,
  so op 134 on frame F times out on frame F + timeout);
- e+0x53CC := 0; the **largest** active threshold (> 0) becomes life and HUD life and is cleared (the others stay);
  main context → timeout sub (`Ecl_SetSub`); timeout := −1; timeout sub := death sub; timer := 0;
- unless flags A bit 27: the card is marked timed out (`0x42BC50`: spell flags bit 2 cleared, bit 3 set, card bonus
  +0xFC := 0 — the card stays **active**; the timeout sub's own op 123 ends it) and `BulletMgr_CancelAll(4)`.
  Neither step tests whether a card is active: a timeout with no card (bit 27 clear) still sets the spell flags and
  cancels all bullets with mode 4 (VERIFIED 0x42B930, 0x42BC50, RE-Q24);
- boss and player state 0 → player state 3 for 70 frames (as above);
- `Enemy_KillChildren(e, no drops)`; non-boss enemies life 0 + death subs (as above); threads freed; descriptor and
  auto-fire reset; rank ranges reset; working depth 0 (no effect); warning level cleared. Return 1 (re-run).

### 10.3 Death (life < 1, flags B bits 3 and 6 clear)
flags B bit 3 set; chained children killed with drops; `GameMgr_GaugeAdd(d, not forced)` with d = −200 when
player byte **+3** (0x17D5EFB, the immediate focus byte — not the +5 mode byte) == 0, else +200 (so also +200
while +3 is 2 right after load); not forced → skipped entirely while bombing (0x17D6ED4 ≠ 0); the gauge is
clamped to [GM+0x3DDF8, GM+0x3DDFA] and the old-value copy (+0x20) updated (changelog/answers_impl14.md §4, VERIFIED
0x42D65C); then by death mode (op 129):

Before the mode switch (all modes, VERIFIED `EnemyMgr_Calc` 0x42D551): flags B bit 3 (death latch) set;
e+0x53CC := (timeout − timer)/60; timeout := −1; **all 4 life thresholds := −1**; threads freed; parent child count −1
if chained; `Enemy_KillChildren(e, with drops)`; gauge ±200 (below).

| mode | behaviour |
|---|---|
| 0 | add score; enemy dead (alive cleared); aura told to end (interrupt 3, then e+0x53C8 := 0); then as "0–1" and "0–2" |
| 1 | add score; flags A bit 23 set, bits 2, 3, 6 cleared (stays alive, e.g. scripted death animation); then as "0–1" and "0–2" |
| 0–1 | if boss: HUD boss display off (Hud +0x2C := 0) and op 128 effects released. No death mode clears the boss table entry (0xF54CC0[slot]) or flags A bit 1: only op 127 −1 and `Enemy_Free` (op 1 delete, off-screen free) do. Mode 0 clears only the alive bit (no `Enemy_Free`), so a mode-0 boss stays in the boss table; timeline op 10 still stops waiting because it also tests the alive bit (changelog/answers_impl20.md §Q2) |
| 2 | no score; stays alive; only "0–2" |
| 0–2 | item drops (§10.4); if boss and **no** card active: `BulletMgr_CancelBonus(8000, 1)` + `EnemyMgr_KillAll(8000)`, sum → score + `Hud_ShowBonus`; life := 0; replay-stats word (0x18B8A28)+0xDA \|= 0x20 |
| 3 | life := 1; flags A bit 3 (loses life) and death-mode bits 20–22 cleared (the latch B bit 3 stays until life > 0 again); HUD boss display off (Hud +0x2C := 0) for **every** mode-3 enemy, boss or not (no flag A bit 1 test, unlike "0–1"; VERIFIED 0x42D6AF case 3, RE-Q24); stats \|= 0x20; death effect e+0x3310 ×3 if ≥ 0; aura interrupt 3 and aura pointer e+0x53C8 := 0 (as mode 0); player state 0 → 3 with timer **90**; flags A bits 30 and 31 cleared. No drops, no bullet cancel, no score |

The death latch (flags B bit 3) is set only by the death path above and cleared at one place: in `EnemyMgr_Calc`, right
after step 6 and before the death test, if the latch is set and life > 0 (VERIFIED 0x42D52x; changelog/answers_impl20.md §Q3). Any
way life becomes positive works (op 131, a write to var 10051, a threshold clamp, mode 3's life := 1). The check runs
only on frames where the enemy's script block runs (not chain-killed, flags A bit 10; not paused by flags B bit 7 or the
flags A bit 30 bomb/state condition).

Unless bit 10: sfx 2/3, death effects: `Effect_Spawn(e+0x3310, pos, 1)` if that signed
byte ≥ 0, then `Effect_Spawn(e+0x3311 + 4, pos, 4)` (4 sparks, 4 draws each); then **if the gauge is at human
max or youkai max** (`Gauge_IsHumanMax`: gauge ≤ GM+0x3DDFC, or `Gauge_IsYoukaiMax`: gauge ≥ GM+0x3DDFE) →
`ItemMgr_Spawn(final pos, 7 = time orb, 1)` → forced mode 3 → Rng_FloatRange + Rng_SignedRange = **4 draws**
(08 §8.3; nothing if pos.x ∉ [−64, 448] or the item cursor slot is busy). RNG order on a normal kill frame:
drops (`Enemy_DropItems`: its spark effect ×3 for an item, + random extras) → death sparks ×4 → time orb
(changelog/answers_impl14.md §3, VERIFIED at runtime). Then, if a death sub is set: rank ranges reset, working depth 0 (no effect, §5.4), thresholds and timeout −1, threads
freed, descriptor reset, auto-fire interval 0, main context → death sub (clock 0), death sub := −1. Death happens in
step 6, after this frame's `Ecl_Run`, so the death sub's time-0 instructions run on the **next** frame (one frame later
than a threshold switch; changelog/answers_boss_phases.md §2).

### 10.3a `Enemy_KillChildren(e, reward)` 0x42ADB0 — VERIFIED

Two independent blocks, in this order, then `Enemy_ChainUnlink(e)` always (changelog/answers_impl12.md §1–4):
1. **Chain block** when n = `Enemy_ChainChildCount(e)` ≠ 0 (only a chain root: parent +0x2DA4 == 0 and +8 ≠ 0;
   a chained child counts 0 and never chain-kills its later siblings). The boss part (step 5 below) is gated only
   on reward ≠ 0, not on the boss flag (flag A bit 1 only selects the cancel type 7/9).
2. **Single block** when e has a parent and reward ≠ 0.
Per child the order is: cancel type → c from the **current** n → if bombing n := n/3 → popup with the divided n →
c orbs (so c uses n before this child's division; the colour is chosen once from the undivided n).
Parent counter +0x3380: +1 per child created (ops 90–92), −1 in `EnemyMgr_Calc` when an enemy **with a parent**
dies by damage (just before its `Enemy_KillChildren(e, 1)`); nothing else writes it (never reset by a chain kill,
not decremented for freed / timed-out children). Starts at 0 (slot zeroed at creation).

Chain links: e+4 = prev, e+8 = next (parent+8 → first child → …); child +0x2DA4 = **parent pointer** (set by
ECL ops 90–92, which also increment parent +0x3380 and link the child, §10.5). 0x41FD20 = `Enemy_HasParent`.

Chain-kill path (e has children):
1. n = `Enemy_ChainChildCount(e)`; colour once: n < 2 → 0xFFFFFFFF, 2–5 → 0xFFFFFFD0, 6–9 → 0xFFFFFFB0,
   ≥ 10 → 0xFFFFFF80.
2. For each child (via +8): if child flag A bit 9 (follow) → child +0x2D40 := parent pos (+0x2D34); set child flag
   A bit 10 (0x400); +0x2DA4 = 0, +8 = 0, +4 = 0.
3. If reward ≠ 0, for each child k:
   - cancel type = 7 if the parent has flag A bit 1, else 9;
   - orb count c: team shots: n < 8 ? 2n + 10 : 26; solo human: n < 4 ? 6n + 16 : 40; solo youkai: n < 10 ?
     2n + 6 : 26;
   - **QUIRK**: if the player is bombing (0x17D6ED4 ≠ 0) n := n / 3 here, inside the loop, so it compounds
     (n/3, n/9, …) and also lowers the following children's c;
   - `Popup_Large`(child +0x2D88 (the old final pos), n, 0, colour); then child final pos +0x2D88 := +0x2D34 +
     +0x2D40;
   - `Player_AddCancelCircle`(pos, 32.0, 2.0, 8, type);
   - c times: r = `Rng_FloatRange(2c)`, then a = `Rng_SignedRange(π)` (**2 RNG draws**, r first); orb at pos +
     (cos a·r, sin a·r), z 0; `ItemMgr_Spawn`(type 7 time orb, mode 3);
   - if !0x42F1F0(EnemyMgr) or a spell card is active (0x4178A0): child +0x3304 := 8 and `Enemy_DropItems`(child, 0);
   - `Sound_PlayAt`(2 + (k & 1), pos).
4. Always for each child: +0x330C = +0x3308 = 0, +0x3304 = −2.
5. Boss part (reward ≠ 0): manager scale (2, 2); `Popup_Large`(boss +0x2D88, boss +0x3380, 0, 0xFFF0F00F);
   scale (1, 1); then 2·(+0x3380) times: r = `Rng_FloatRange(128)`, a = `Rng_SignedRange(π)` (r first), pos =
   boss +0x2D88 + polar(a, r), z 0, `ItemMgr_Spawn`(type 7, mode 1); then `Player_AddCancelCircle`(boss pos, 32,
   1.0, 16, 7) and `Timer_Set`(0x18B89D4, 0).

Single-kill path: taken when e has a parent and reward ≠ 0 (a chained child killed on its own; `EnemyMgr_Calc`
first does parent +0x3380 −= 1): gauge −= gauge/12; timers 0x18B89E0 = 0, 0x18B89C8 = 30, 0x18B89D4 = 50; final
pos recomputed; `Popup_Large`(pos, 1, 0, 0xFFFFFFFF) ("+1"); `ItemMgr_Spawn`(type 7, mode 1); drops cleared; then
unlinked from the chain (0x42B2F0).

Callers: `EnemyMgr_Calc` death (reward 1), `Enemy_CheckLifeThresholds` (1), `Enemy_CheckTimeout` (0),
`Enemy_Free` (0), `Player_UpdateBomb` dissolve end (0).

### 10.4 Drops (`Enemy_DropItems` 0x42BEA0)
item ≥ 0: `Effect_Spawn(e+0x3311 + 4, final pos, 3)` (3 sparks), then spawn item `item` at final pos.
item == −1: if counter (u16 mgr+0x9DCDC0 = 0xF54CE0) % 3 == 0 → `Effect_Spawn(e+0x3311 + 4, final pos, **6**)`
and spawn the item from the 32-entry byte table 0x4C70D8 at index (u16 mgr+0x9DCDC2 = 0xF54CE2; then += 1,
wraps 32 → 0); **then counter += 1 in every case** (check before increment). Both values are set at every stage
load by `EnemyManager_Load`: counter = `Rng_Range16(3)`, index = `Rng_Range16(8)` (the 2 RNG draws of 11 §3
table); nothing else writes them (VERIFIED, changelog/answers_impl16.md §5). An item −1 death that does not drop spawns no
sparks. The item table:
`00 00 01 00 01 00 00 00 01 01 00 00 01 01 01 00 01 00 01 00 01 00 01 00 01 00 00 01 01 01 00 00`.
item ≤ −2: nothing (no sparks, counter unchanged). Then e+0x330C power items (type 0 if player power < 128 else 1) and e+0x3308 type-1 items
at final pos + (Rng·128 − 64, Rng·128 − 64); counts reset. (Item types: 08 spec.)

### 10.5 Child chains (ops 90–92)
Spawn fails (manager flag) if the parent's life < 1 or it is being chain-killed. The new enemy (vars copied,
§4.1; its time-0 instructions already ran inside the spawn — so a child placed in a higher slot than the slot being processed runs `Ecl_Run` **twice** in its spawn frame and a `wait n` started at time 0 fires at spawn frame + n − 1; VERIFIED demo 2: wait 30, spawn 2250, shot 2279) is linked after the chain tail: if the spawner is a chain root, the tail found by following +8; otherwise (a root without children yet, or a chained child spawning) **the spawner itself** — a chained child that spawns links the new enemy directly after itself and overwrites its own +8, cutting the rest of the chain off the root's walk (no data case known). Parent pointer +0x2DA4 = the spawner (not the root); spawner +0x3380 += 1. All of the following is applied **after** the spawn-time `Ecl_Run`, so it overrides the child's own time-0 ops (e.g. ecldata3 sub16 `flags_on 3` sets bit 2, then bit 2 is cleared here: escan 0x01001149, changelog/answers_impl21.md §Q5); it gets
bit 8 (reacts to player mode), bit 11 = current mode (player byte +5), layer 2 (human, mode 0) / 0 (youkai),
contact damage off, and — only if it has none yet (e+0x53C8 == 0) — an aura `Effect_SpawnHigh(0x20, pos, 1)`
(pos e+0x2D34; op 92: e+0x2D88) with `AnmVm_Interrupt` **1 when mode == 0 (human), 2 when youkai**; aura VM flag
bit 17 := flags A bit 2 (just cleared → 0); if e+0x2E0C bit 0 the aura VM +0x14 is negated (changelog/answers_impl14.md §7,
VERIFIED 0x41E5ED…). The chain link itself (child +0x2DA4 := spawner, tail +8 := child, child +4 := tail, spawner +0x3380 += 1) is written last, after the aura, so during the spawn-time `Ecl_Run` the child has no parent and is not in the chain (var 10096 reads 0 there, §6); the tail is looked up before the spawn (VERIFIED op 90 code, RE-Q24). Op 91 position relative to the parent's final pos; op 92 additionally makes the
child follow the parent (offset = parent pos, bit 9). Sound 0x24. When the parent dies or passes a threshold,
children are killed (bit 10) with score popups / item showers proportional to chain length (0x42ADB0).

### 10.6 Enemy drawing — VERIFIED (0x42E140; draw prio 8 → layers 0–1, prio 11 → layers 2–3)
Enemies are linked each calc (prepended, §10.1 step 7) into 4 lists by the layer byte e+0x332F (template 0; op 159 = value, op 156 = 2,
mode-reacting enemies 0 in youkai / 2 in human mode). Prio 11 draws with the global tint 0xFFF01010 while
bullets are frozen (0x164D0B4 bit 10). Each list is walked from its head, i.e. in reverse processing order (highest linked slot first). Per enemy, all with Draw2DRot; positions + playfield origin (0x164D2DC = 32, 16):
1. sub VM 0 (e+0x2B0, if its script ≥ 0): rotation = enemy angle (e+0x2D94) if the VM tag ≠ 0; pos = final pos
   + (flags B bit 8 ? main VM pos offset : its own pos offset), z 0.3;
2. main VM (e+0xC): rotation = angle if flag A bit 25; pos = final pos + main VM pos offset (+0x288), z 0.25;
   optional after-image trail (e+0x534C flags, ring buffer at e+0x3394…, op 157) drawn first;
   **Trail draw** (VERIFIED `EnemyMgr_DrawLayers` 0x42E140, RE-Q24), only while flags F = e+0x534C ≠ 0; n = e+0x534E,
   step b = e+0x5352; entries E[i] = ring entry i (pos +0, angle +0x18, §10.1); an entry is used only if E[i].x ≥ −990.
   The main VM's scale x/y and colour are saved first and restored after the trail.
   - F bit 3 (8) clear — sprite copies: for i = n − 1, n − 1 − b, … while i > 0 (entry 0 is not drawn here): if flag A
     bit 25: rotation z := E[i].angle; if F bit 1 (2): scale.x := sx − i·sx/n; if F bit 2 (4): alpha := a − (a·i)/n
     (integer); pos := E[i].pos + main VM pos offset + playfield origin, z 0.3; `Draw2DRot`(main VM). Oldest first.
   - F bit 3 set — one triangle strip (`Anm_DrawRingStrip`, vertex buffer e+0x3E14): walk i = 0, b, 2b, … while i < n
     and E[i].x ≥ −990, counting 2 vertices per entry (k). If k > 2: texture coordinate A runs from sprite +0x28 + VM +0x30
     down by (sprite +0x28 − sprite +0x20)/((k+1)/2 − 1) per entry (both vertices of an entry share it); per entry, angle θ = E[0].angle for i = 0, else
     mid(E[i−1].angle, E[i].angle) (index i−1, the adjacent history entry, even when b > 1; VERIFIED RE-Q26 disassembly 0x42E64E) with mid(a, c) = lo + 0.5·d where lo = min, d = the smaller of |a − c| and
     lo + 2π − hi (QUIRK: takes the wrap distance from lo, not the true shortest-arc midpoint); half width
     h = scale.y·sprite height (+0x30)/2, shrunk by (1 − i/n) if F bit 1; the two vertices are E[i].pos + (32, 16) +
     (−sin θ·h, cos θ·h) and + (sin θ·h, −cos θ·h), coordinate B = sprite +0x24 + VM +0x34 for the first vertex and sprite +0x2C + VM +0x34 for the second,
     colour = VM colour with alpha a − a·i/n if F bit 2. With F bit 1, an interior entry (0 < i, i + b < n) is
     skipped (k −= 2) when |θ_prev − θ| < 1e−5 **and** |θ − m| < 1e−5, where θ_prev = θ of the last entry that was
     emitted and m = mid(E[i + b − 1].angle, E[b].angle) (QUIRK: E[b] is a fixed index, not E[i + b]) (VERIFIED RE-Q26,
     0x42E6C0–0x42E72A). Drawn if k > 2 after the walk.
     **Unreachable with retail data**: op 157 is used 22 times with flags 0 or 5 (bits 0 and 2); F bit 3 never occurs.
   - The main VM itself is then drawn unless F bit 4 (0x10) or flag A bit 5.
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
out (0x42BC50, from `Enemy_CheckTimeout`), bit 5 last spell (:= GameFlags bit 7 at `Spell_Start`, i.e. op 176 ran before the card; failing such a card shows message 6 "Last Spell Failed", changelog/answers_impl10.md §7), bit 7 "bomb started during this card" (bomb damage
allowed, §10.1), bit 8 capture payout pending, bit 9 captured, bit 10 name-only declaration (set by native 22 `Spell_ShowNameOnly` 0x415D10, which also
clears bits 0 and 4 and calls `Gui_ShowBossSpellName(face −1, fixed exe string 0x4B495C, sprite 1)`: no portrait / cut-in, the spell-background VM +0x2374 gets no interrupt 1, only the name plate, text VM and plate interrupt play, SE 0xE, panel repaint 2; bits 2/3 untouched, so a later `Spell_End` sees bit 0 clear and does nothing; the bonus/history display is skipped; used by Mokou's
「リザレクション」 in ecldata8/8sp), bit 11 bonus frozen (op 184 `Spell_SetBonusFrozen` 0x41F0E0 = n & 1; no
decay in `Spell_Calc` 0x416B90 and time-orb +8000 in `Spell_AddBonus` 0x416B10 ignored; all 82 uses are
`spell_ex 1` at the start of boss death subs). Both cleared by `Spell_Start`; bit 11 also by `Spell_End`.
Bit 6 is set by op 164 (`Spell_SetFlag6` 0x41F0B0). Bonus +0xFC.
On op 123 with the card active: bit 0 off; if not timed out → `CancelBonus(8000, 1)` + `KillAll(8000)` →
score + `Hud_ShowBonus`; if capture still possible → captured: payout value = +0xFC, time reward 700 (flag A
bit 27) or from the remaining card time: D = card timer +0x114 (the duration at start), thr = D − D/7, r = remaining +0x108; r ≥ thr → 1000, r < 180 → 100, else (r − 180)·900/(thr − 180) + 100 (integer), captures counter, score-file
stats, checksum (RNG), start-ring recoloured, SE 0x23, bits 8 and 9; the ring pointer moves from +0xF4 to +0xF8.
**The time reward (g_Gui +0x10) is paid as time orbs, not by AddTime** (VERIFIED 0x416B90; golden trace: the
payout frame changes the score but not stats+0x3C, the time rises orb by orb before it; RE-Q24). `Spell_Calc`
(skipped while GameFlags bit 10 or the freeze byte is set), while the card is inactive and +0xF8 ≠ 0, per frame
with t = the ring's timer +0x338:
- t == 30, t == 60: ring restyle steps (VM timers/targets; no gameplay effect); t == 130: payout (below);
- centre +0x2E0 += (player pos 0x17D61AC − centre)/4 if t > 80, else /16 (z := 0); rotation +0x318 += −0.0524
  (t > 80) or −0.0157; +0x32C := VM +0x20C;
- if t > 8 and +0x10 > 0: angle = Angle_AddNormalize((t − 10)·2π/40 − π/2) (t as float); k = min(+0x10, 7);
  k × `ItemMgr_Spawn`(centre + polar(angle, 128), type 10, mode 0); +0x10 −= k; then at angle + π (normalised):
  k' = min(+0x10, 7), but **6** spawns whatever k' is (quirk), +0x10 −= k'; +0x10 := max(+0x10, 0). Type 10
  becomes a time orb (type 7, mode 5, 08 §8.3); the time arrives when the orbs are collected.
Payout at t == 130: ring freed, +0xF8 := 0, `Hud_ShowSpellBonus`(+0x104), AddScore(+0x104), bit 8 off; only if
+0x10 > 0 still: AddTime(+0x10), stats+0x24 (max point) += +0x10·10, +0x10 := 0 (normally +0x10 is already 0).
Or, if bit 8 is still set at the next `Spell_Start`: +0xF8 := 0 (the ring is not freed), ShowSpellBonus, AddScore,
bit 8 off, and if +0x10 > 0: AddTime(+0x10) (no max-point add), +0x10 := 0.
GameFlags bits 7–8 (last-spell / dissolve mode): captured → if bits 7–8 ≠ 0 they become "bit 8 only"; not captured → cleared. Not captured → HUD message 5 "Spell Bonus Failed" / 6 "Last Spell Failed" (5 + bit 5). Always: player bomb cooldown +0xE2A6C := 16 (09 §1.1; no normal bomb for 16 player frames); player in state 0 → state 3 for 70 frames; SE 0xF; stage
spell background off.

## 12. Native functions (op 136 once, op 137 every frame) — VERIFIED behaviour summary

Called with the enemy and the instruction; `arg` = second operand (instr +0x10).
| n | behaviour |
|---|---|
| 0 | globals 0x4E3D28 = var 10000, 0x4E3D24 = var 10016 |
| 1, 17 | screen flash (type 3: duration 60 (native 1) / 180 (native 17), 1 repeat, 0xFFFFFFFF; prio 0x15; 13 §3) |
| 2 | bounce: vel.x negated at x ≤ 0 / ≥ 384; vel.y += var 10022 while vel.y < var 10023; vel.y negated when y < −64; clears bit 28 when y ≥ 480; angle = atan2(vel) when changed |
| 3, 6, 20 | create two effects of the same type (native 3: 0x38, 6: 0x3A, 20: 0x41; colour 0xFFFFFFFF at the enemy) in fixed slots 9 and 10; the slot-10 effect then gets etama script 0x61 / 0x65 / 99; set the stage callback 0x4235A0 (§12.1). Native 5 ends both |
| 4, 7, 21 | bullet "warp" across rectangular zones (zone constants 124/260/140/276 & 56/328/72/344; 56/328/88/360 & −32/416/0/448; 112/271/128/287 & 33/350/49/366) with scaling about the centre; bullets reverse direction |
| 5 | end effects 9 and 10; global 0x4E4B60 = 2 |
| 8 | familiar ring: chain members with the same tag (var 10038) get an index (var 10037) and count (var 10018, change flag var 10017); each turns its orbit angle toward leader + index·2π/count by 2 % of the difference |
| 9, 11, 25 | wide horizontal player-hit rectangles (590 × 160/128, 240/192, 288/224) at the boss with a 12-frame warning phase |
| 10 | flash (type 3: duration 30, 5 repeats, 0x40FFFFFF) + shake (type 7: amplitude 4, ramp 120, hold 190, out 60) (13 §3) |
| 12, 14 | toggle bullets whose flags match var 10000 between two states (sprite ±16, speed from var 10016/10017); 12 also walks the calling enemy's child chain (e+8 = next: parent+8 → first child → child+8 …, e+4 = prev): var 10001 == 0 → set flags B bit 7 on every chained child and send interrupt 2 to the two spell-background VMs (0x4E4B68 / 0x4E4E0C); else clear bit 7 and send interrupt 1 (changelog/answers_impl10.md §4, VERIFIED) |
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
| 31 | `ItemMgr_Spawn`(e pos +0x2D34, type, mode 0): type 5 (1up), or type 3 (bomb) while the player is bombing (player +0xFDC = global 0x17D6ED4 ≠ 0) at the moment of the call (VERIFIED 0x425390; used once: ecldata8 sub30 time 0) |


### 12.1 Spell mask callback of natives 3 / 6 / 20 — VERIFIED

**Stage callback** `Stage_SpellMaskCallback` 0x4235A0 is stored in `g_Stage`+0x625C (0x4EA28C).
  `Stage_DrawHigh` calls it after drawing the spell-background VMs, only while `g_Stage`+0xB24 > 0.
  `Spell_Start` clears it. Steps:
  1. a = effect 9's VM pos.x · √½ and b = effect 10's VM pos.x · √½. These are the ring radii (update
     0x4279D0: radius := pos.x), so the squares are inscribed in the rings. c = effect 9 pos + (32, 16).
  2. Draw a 10-vertex triangle strip, opaque **black** (0xFF000000), z 0.8, rhw 1, with FVF XYZRHW|DIFFUSE.
     It is a square frame between the square of half-size a and the one of half-size b around c; vertex
     order (c−a), (c−b), (+a,−a), (+b,−b), (+a,+a), (+b,+b), (−a,+a), (−b,+b), back to the start.
     Render state:
     - colour/alpha op SELECTARG1 with arg DIFFUSE (no texture);
     - z-write on;
     - ZFUNC unchanged (ALWAYS).
  3. Restore texture stage 0 to MODULATE / TEXTURE, invalidate the ANM state caches, and set ZFUNC
     LESSEQUAL.
  4. Redraw spell-background VM 0 (`g_Stage`+0xB38) mirrored:
     - scale (−1.5, −1.75), pos (416, 464, 0.7), colour 0xFFE0C0C0, `Draw2DRot`;
     - then restore scale (1.5, 1.75), pos (32, 16, 0.5) and the colour.
  5. Redraw VM 1 (+0xDDC):
     - rotation z negated, pos.z 0.6, colour 0xFFE0C0C0, `DrawRotFlush`;
     - then rotation negated back, pos.z 0.5, colour restored.
  6. ZFUNC ALWAYS.

With a depth buffer and these states the picture follows from the depth test (z 0.8 frame, 0.7/0.6 mirrored copies, ZFUNC LESSEQUAL).

## 13. Verification

`python3 -I tools/ecldis.py extracted/dat -q` → coverage of sub ops 0–184 present in data, timeline ops,
"unknown ops: [] []", "problems: 0" (sizes vs. signatures, jump targets, terminators, sub ids). Spell names
decode to valid Shift-JIS text (e.g. stage 1 boss spells), confirming the XOR keys.

## 14. Open points
- Exact semantics of the bullet extra commands (op 111), laser fields and item types belong to 08.
- Player-side functions used here (damage computation, hit rectangles, mode byte 0x17D5EF8+5) → 09.
- HUD/dialogue functions (timeline 6/7, ops 148/158/179–181) → 10/11.
- Flag A bits 7, 23, 25, 30, 31: set by ops but consumers only partly traced.
