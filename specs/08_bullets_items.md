# 08 — Bullets, lasers, items

Sources: `BulletMgr_Calc` 0x431240, `BulletMgr_PlaceBullet` 0x42F5F0, `BulletMgr_SpawnPattern` 0x430E10,
`Bullet_ApplyExCmds` 0x42FFC0 + motion helpers 0x432210–0x432AA0, `BulletManager_Load` 0x433070,
laser spawn 0x430F20, cancels 0x430830 / 0x430AA0 / 0x430D30, `BulletMgr_Draw` 0x432B50,
`ItemMgr_Spawn` 0x4400A0, `ItemMgr_Calc` 0x440500, collection handlers 0x440CF0–0x4412B0.
Tool: `tools/bullettypes.py` (type table → etama.anm scripts, sprite size, hitbox; checked against the exe table).
Everything is VERIFIED from code unless marked INFERRED. Units: pixels, frames, radians. "gs" = game speed
(`g_GameSpeed` 0x17CE8E0, normally 1.0). Playfield coordinates: (0,0)–(384,448); drawn at +(32,16)
(globals 0x164D2DC / 0x164D2E0).

## 1. Managers and update order

| object | address | notes |
|---|---|---|
| bullet manager | 0xF54E90 | calc priority 14, draw priority 13 (funcChain) |
| item manager | 0x1653648 | updated from inside the bullet manager calc, drawn from its draw |
| player | 0x17D5EF8 | collision functions (09 spec) |
| game manager | 0x160F508 | score/rank/power/etc., `+8` → stats block (09 spec §9) |

`BulletMgr_Calc` per frame (skipped completely when global 0x164D0B4 bit 10 is set):
1. `ItemMgr_Calc` (§8).
2. Live-bullet counter := 0; size-class draw lists cleared (§2.3).
3. Bullets, visiting slots in the order **0, 1535, 1534, …, 1** (each once). For each non-free bullet: counter += 1,
   then the state machine (§4).
4. Lasers, slots 0..255 (§6).
5. Cancel-guard counter (mgr+0x6BA53C) decremented if > 0; manager timer (+0x6BA540) ticked; frame counter
   (+0x6BA54C) += 1.

Draw: items (§8.6), then lasers, then bullets by size class 0 → 5 (each class list in reverse visiting order:
the list is a LIFO built during calc), then the effect manager.

Per-bullet draw (`Bullet_Draw` 0x432F20, VERIFIED RE-Q24): the VM is chosen by the state +0xDB8 — states 2–5 use the spawn-effect VMs at +0x2A4·(state − 1) (+0x2A4, +0x548, +0x7EC, +0xA90), any other state the main VM at +0; that VM's pos := bullet pos (+0xD44) + playfield origin (0x164D2DC/E0), z := 0.05; colour RGB := 0xFFFFFF keeping the VM's alpha; if the VM tag +0x1FC ≠ 0: rotation z := Angle_AddNormalize(π/2 + bullet angle +0xD74); then `AnmManager_Draw2DRot`.

## 2. Bullet manager layout

| offset | size | content |
|---|---|---|
| +0 | 21 × 0xD44 | bullet templates (one per type) |
| +0x1A880 | 1536 × 0x10B8 | bullets (0xF6F710); slot 1536 is a sentinel with state 6 |
| +0x660938 | 256 × 0x59C | lasers |
| +0x6BA538 | i32 | live bullet count (from the last calc; also global 0x160F3C8) |
| +0x6BA53C | i32 | cancel guard: set to 10 by every mass cancel; while > 0 new bullets start in state 5 and lasers do not spawn |
| +0x6BA540 | Timer | ticked every calc |
| +0x6BA54C | i32 | frame counter |
| +0x6BA554 | ptr[6] | draw list heads per size class |
| +0x6BA56C | ptr | allocation cursor |
| +0x6BA570 | i32 | item type dropped by ordinary cancels = 6 (set in the ctor, never changed) |
| +0x6BA574 | ptr | etama.anm (ANM slot 6) |

### 2.1 Templates (`BulletManager_Load`)
Template t (0..20) holds 5 ANM VMs started with the scripts from the exe table 0x4B4AD8 (5 ints per type):
VM0 main sprite (+0), VM1 spawn effect "flag 2" (+0x2A4), VM2 "flag 4" (+0x548), VM3 "flag 8" (+0x7EC),
VM4 cancel animation (+0xA90). All 5 get VM flag 0x2000. Template extras:

| off | type | meaning |
|---|---|---|
| +0xD34 | f32 ×2 | hitbox width, height (equal) |
| +0xD3C | f32 | 0 |
| +0xD40 | u8 | 0 |
| +0xD41 | u8 | trunc(sprite height) |
| +0xD42 | u8 | size class (draw order bucket) |

Hitbox / class from the main sprite height h and the type's main script number s:
- h > 32: box 24, class 0.
- 16 < h ≤ 32: s ∈ {8, 113, 114, 115} → box 5, class 2; s ∈ {9, 109, 110} → box 8, class 1; else box 10, class 1.
- 8 < h ≤ 16: s ∈ {2, 4, 5, 6, 106, 107, 108, 111, 112} → box 4, class 4 (111/112 were missing here; the table below was right — VERIFIED 0x433070, RE-Q30, fullrun_noclip 78238) (s = 5 falls through to the next case in the
  original; net result box 4 class 4); else box 6, class 3.
- h ≤ 8: box 4, class 5.

Resulting table (`tools/bullettypes.py`, etama.anm sprite sizes):

| type | main script | sprite (w×h) | hitbox | class |
|---|---|---|---|---|
| 0 | 0 | 8×8 | 4 | 5 |
| 1 | 1 | 16×16 | 6 | 3 |
| 2 | 2 | 14×16 | 4 | 4 |
| 3 | 3 | 16×16 | 6 | 3 |
| 4–6 | 4–6 | 14×16 | 4 | 4 |
| 7 | 7 | 32×32 | 10 | 1 |
| 8 | 8 | 32×32 | 5 | 2 |
| 9 | 9 | 32×32 | 8 | 1 |
| 10 | 25 | 64×64 | 24 | 0 |
| 11–13 | 106–108 | 16×16 | 4 | 4 |
| 14, 15 | 109, 110 | 32×31 | 8 | 1 |
| 16, 17 | 111, 112 | 16×16 | 4 | 4 |
| 18–20 | 113–115 | 32×32 | 5 | 2 |

Effect scripts per type: types 0: (18,19,20,15); 1–6, 11–13, 16, 17: (21,22,23,16); 7–9, 14, 15, 18–20:
(24,24,24,17); 10: (27,27,27,26).

### 2.2 Bullet object (0x10B8)
The first 0xD44 bytes are a copy of the template (5 VMs + extras); then:

| off | type | name | meaning |
|---|---|---|---|
| +0xD44 | f32[3] | pos | z = 0.1 at spawn |
| +0xD50 | f32[3] | vel | per-frame displacement (already × gs) |
| +0xD68 | f32 | speed | |
| +0xD74 | f32 | angle | |
| +0xD80 | Timer | state timer | reset on entering state 1 |
| +0xD8C | Timer | age | used for the 16-frame graze delay |
| +0xDA8 | i32 | offscreen grace | frames during which the off-screen check is skipped (ex cmd 0x2000) |
| +0xDAC | u32 | active | bits of running ex commands |
| +0xDB0 | u32 | flags | descriptor flags (ECL bullet instruction +0x28) |
| +0xDB4 | i16 | colour | |
| +0xDB8 | i16 | state | 0 free, 1 normal, 2/3/4 spawn effect, 5 cancelling, 6 sentinel |
| +0xDBA | i16 | offscreen counter | (§4.3) |
| +0xDBC | u8 | 1 | |
| +0xDBD | u8 | grazed | |
| +0xDBE | u8 | cancel-after-spawn | set while in a bomb area during a spawn effect |
| +0xDC0 | ptr | draw list link | |
| +0xDC4 | i32 | 0 | |
| +0xDC8 | i32 | ex sfx | sound played by ex commands (descriptor +0x204; −1 = none) |
| +0xDCC | i32 | ex index | next ex command slot (descriptor +0x208, normally 0) |
| +0xDD0 | 18 × 24 | ex commands | copy of the descriptor's command slots (§5) |
| +0xF80 | 7 × 0x2C | ex state blocks | §5.2 |
| +0x10B4 | u8 | intangible | no graze/hit while 1 (ECL natives 12/14, §7) |

### 2.3 Size-class lists
After a bullet's update (any state except when freed this frame) it is pushed on the list of its class
(`link = head[class]; head[class] = bullet`). Draw walks class 0..5.

## 3. Spawning

### 3.1 From ECL (link: 07 §9)
`BulletMgr_SpawnPattern(desc)` (the ECL descriptor, 0x210 bytes, 07 §9 / structs.md):
1. Sets bit 0x800 of a replay/statistics word (global ptr 0x18B8A28, +0xDA).
2. If the live count from the last calc ≥ 1536: nothing.
3. desc+0x20C := template[desc.type]; aim = `Player_AngleFrom(desc.origin)` (= atan2(player.y − y, player.x − x),
   or π/2 when both differences are exactly 0).
4. For j = 0..count2−1, i = 0..count1−1: `PlaceBullet(desc, i, j, aim)`; stop everything as soon as one fails.
5. If desc flags & 0x200: play sound desc+0x200 panned at origin.x.

### 3.2 PlaceBullet
Slot search: from the cursor, up to 1536 slots, skipping non-free ones; reaching the sentinel wraps to slot 0.
No free slot → failure. Speed and angle: 07 §9.3 (mode table). Then:
- state 1; +0xDBC = 1, grazed = 0, timers reset, intangible = 0; speed; stored angle +0xD74 = wrap(a)
  (`Angle_AddNormalize`); pos = desc origin with z = 0.1; vel = (cos a, sin a, 0) × (speed × gs) with the
  **unwrapped** pattern angle a, not the stored one (VERIFIED 0x42F97E pushes the local angle to `Vec3_FromPolar`,
  RE-Q30): for a ≈ 4π/3 the f32 velocity differs in the last bits from polar(wrap(a)) (fullrun_noclip row 29948).
- VM0 and VM4 copied from the template; hitbox, +0xD3C..+0xD42 copied; ex sfx = desc+0x204; offscreen
  grace 0.
- Colour: if VM0's sprite index ≠ template sprite + colour, VM0 sprite := template sprite + colour.
  VM4 (and the effect VMs below) use a reduced colour set: sprite := template sprite + k where, by the main
  sprite height h, k = colour (h > 32), table 0x4C7138[colour] (16 < h ≤ 32: `0,1,1,2,2,3,4,0`), or table
  0x4C70F8[colour] (h ≤ 16: `0,1,1,1,1,2,2,2,2,3,3,3,4,4,4,0`).
- Spawn effect: flags & 2 → state 2 with VM1; else & 4 → state 3 with VM2; else & 8 → state 4 with VM3. The
  effect VM is copied from the template, recoloured as above, and **pos −= 4·vel**.
- Ex commands copied (18 slots); flags = desc flags; active = 0; ex index = desc+0x208; then
  `Bullet_ApplyExCmds` runs once immediately (§5).
- If the cancel guard (mgr+0x6BA53C) > 0 and flags lacks 0x1000: state := 5 (the bullet only plays its cancel
  animation).
- Cursor := next slot (sentinel → slot 0).

## 4. Per-frame bullet update

### 4.1 State 1 (normal), in order
1. `Bullet_ApplyExCmds` (§5).
2. If active ≠ 0, run the motion handlers in this order for each set bit: 0x1, 0x10, 0x20, 0x40, 0x100, 0x80,
   (0x400|0x800 bounce), 0x400000, 0x800000, 0x20000 (§5.3).
3. If offscreen grace > 0: grace −= 1.
4. Unless the global freeze byte 0x160F534 is set: pos += vel.
5. Off-screen handling (§4.3) only when grace == 0 (re-tested after step 3).
6. Collision, only if intangible == 0:
   - if not yet grazed and age > 15: g = `Player_GrazeTest` (box = hitbox enlarged by 20 on every side vs the
     player's graze box; 0x44A470: the bomb-area test runs first → 2; otherwise no graze while the player is in
     state 1 or 2, graze allowed in states 0 and 3). g == 1: grazed := 1 (the graze handler already ran, 09 §6), continue to the hit test.
     g == 2 (inside a bomb area): if flags lacks 0x1000 → state 5 + bomb drop (below); no hit test.
     g == 0: no hit test this frame.
   - Box arithmetic of both tests (VERIFIED 0x44A230 / 0x44A360 / 0x44A470, RE-Q30): object box min = (p − s/2) − e, max = (s/2 + p) + e (e = 20 for the graze test, 0 otherwise; f32), compared with the player's stored box (hit box +0x38C/+0x398, graze box +0x3A4/+0x3B0, 09 §1.1): miss iff obj max < player min or player max < obj min on x or y (touching edges overlap). `Player_HitTestNoBomb` (enemy contact) uses the same form with e = 0.
   - hit test h = `Player_HitTest` (hitbox vs the player's hitbox): h == 1 → state 5 (the player is hit, 09 §7);
     h == 2 → if flags lacks 0x1000: state 5 + bomb drop.
   - Bomb drop: item type = player+0xE2A90: 9 → two items of type 7; ≥ 0 → one item of that type; −1 → none.
     Item mode 1 (attracted). **+0xE2A90 has exactly 4 writers** (VERIFIED RE-Q27, displacement scan):
     `Player_InBombArea` 0x449FF0 writes the area's item **only on a hit**; `Player_HitTest` 0x44A230,
     `Player_HitTestNoBomb` and `Player_GrazeTest` 0x44A470 write 6 at their start, before their own bomb-area test.
     The state 2–4 mark test (§4.2) and the cancels (`BulletMgr_CancelAll` / `CancelBonus`) call `Player_InBombArea`
     directly, so a miss there **keeps the previous value** (the item of the last hit by any bullet, this frame or
     earlier). Runtime demo 0 f4451: bullet 182 drops at its effect end with +0xE2A90 = 7 left by bullet 181's
     hits (area 199, item 7), although 182 itself is in no area that frame → a time orb, not a star.
7. If VM0 has a script: run VM0.
8. Tick state timer and age; push on class list.

### 4.2 States 2, 3, 4 (spawn effect)
age.Decrement (so the age does not advance net); pos += vel / 2 (state 2), / 2.5 (state 3), / 3 (state 4);
if flags lacks 0x1000 and `Player_InBombArea(pos)` → +0xDBE = 1 (sticky: tested on every spawn-effect frame and
never cleared). Run the effect VM. When it finishes:
if +0xDBE: bomb drop as above (the state is set to 5 but immediately overwritten — the bullet survives; faithful
quirk); state := 1, state timer reset, and the state-1 update runs in the same frame — whose collision test can
hit the bomb area again (state 5 + a second drop). Runtime demo 0 f4451 (changelog/answers_impl21.md §Q4): bullets 181/182/183
end state 2 marked → 5 time orbs (181 and 183 two each, 182 one and it stays in state 1). Otherwise tick timers,
push on list.

### 4.3 Off-screen rule
On-screen test (`GameMgr_IsOnScreen` 0x4399AC): box centred on pos with the **current sprite's** width/height
(VM0 sprite record +0x34 / +0x30) overlaps [0, 384] × [0, 448] (inclusive). On screen: counter (+0xDBA) := 0.
Off screen and active & 0xDC0 == 0 (no redirect/bounce running): if counter > 0 → counter −= 1, keep; else free.
Off screen with such a command active: counter += 1; free when it reaches 128. The counter is not reset when a
slot is reused (a new bullet that spawns off screen can inherit a leftover count — faithful).
Freed bullets are not pushed on the draw list and skip VM/timer work this frame.

### 4.3a Live-bullet counter (mgr+0x6BA538 = 0x160F3C8) — VERIFIED (0x431240, runtime changelog/answers_impl14.md §2)
`BulletMgr_Calc` zeroes the counter, then for each slot 0..1535 in order: state ≠ 0 → counter += 1 **before**
the bullet's update. So a bullet freed during this frame's update (off-screen rule, cancel end) is still
counted this frame, and a bullet spawned this frame is counted only if its slot is processed after the spawn.
Lasers are not counted. While bullets are frozen (GameFlags bit 10) the whole calc is skipped and the counter
keeps its last value. Readers: `BulletMgr_SpawnPattern` (≥ 1536 → no spawn), GameMgr_Calc slow mode
(11 §4 item 10) and the trace column `bullets`. Example (demo 1, frames 548–560, stage 4A): enemies at
x = 350/400 place 4 bullets at (400, 64) moving right; they leave state 2 at x ≈ 402 and are freed the same
frame by §4.3 — the original frees them exactly like the port; only the count differs.

### 4.4 State 5 (cancel)
pos += vel / 2; run VM4 (cancel animation); when it finishes the bullet is freed. No collision.

## 5. Extra commands (ECL op 111, `Bullet_ApplyExCmds`)

### 5.1 Slot format and scheduling
Slot k (24 bytes, from op 111): `+0 f32 e, +4 f32 f, +8 i32 c, +0xC i32 d, +0x10 u32 cmd, +0x14 i32 chain`.
Loop while ex index ≤ 17:
- cmd == 0 → stop.
- chain == 0 and active ≠ 0 → stop (wait until all running commands end).
- (flags & cmd) == 0 → index += 1, next slot (commands not enabled by the bullet instruction's flags are skipped).
- Execute (table); "→ return" commands do index += 1 and end processing for this frame; "→ continue" ones
  do index += 1 and go on with the next slot.

| cmd | effect at start | then |
|---|---|---|
| 0x1 | active |= 1; block0 timer = 0 | return |
| 0x10 | active |= 0x10; accel = e; angle = f if f > −990 else current angle; dur = c; avec = polar(angle, e × gs); sfx if index ≠ 0 | return |
| 0x20 | active |= 0x20; dspeed = e; dangle = f; dur = c; sfx if index ≠ 0 | return |
| 0x40 / 0x80 / 0x100 | active |= cmd; a = e; newspeed = f if f > −999 else current speed; period = c; count = d; n = 0 | return |
| 0x400 / 0x800 | active |= cmd; bspeed = e if e ≥ 0 else current speed; max = c; n = 0 | return |
| 0x2000 | offscreen grace = c | continue |
| 0x4000 | whole template c copied over the bullet's first 0xD44 bytes (VMs, hitbox, class); VM0 sprite += d | continue |
| 0x20000 | active |= cmd; block5 timer = c (counts down) | return |
| 0x40000 | state := 5 | return |
| 0x80000 | play sound c panned at pos.x | continue |
| 0x400000 / 0x800000 | active |= cmd; block6 timer = c | return |
| 0x1000000 | spawn a sub-pattern (below); uses this slot and the next | continue, or state 5 + return if bit 31 |
| other (2, 4, 8, 0x200, 0x1000, …) | nothing | return |

"sfx" = `Sound_Play(ex sfx)` if ex sfx ≥ 0.

Sub-pattern (0x1000000), slot A = this, B = next: a fresh descriptor (zeroed, +0x204 = −1) with
type = (A.c >> 16) & 0xFF, colour = (A.c >> 8) & 0xFF, aim mode = (A.c >> 24) & 0x7F, ex start index =
A.c & 0xFF, origin = bullet pos, speed1 = A.e, speed2 = A.f, count1 = (i16)A.d, angle1 = B.e, angle2 = B.f,
count2 = (i16)B.c, flags = B.d; ex slots = copy of this bullet's slots. `SpawnPattern` is called (§3.1, the
rank adjustment of 07 §9.2 does not apply). Index += 2. If A.c bit 31: this bullet → state 5.

### 5.2 State blocks (+0xF80 + 0x2C·k)
k0 speed-up {timer}; k1 accel {timer, +0xFB8 accel, +0xFBC angle, +0xFC0 avec[3], +0xFCC dur};
k2 angular {timer, +0xFE4 dspeed, +0xFE8 dangle, +0xFF8 dur}; k3 redirect {timer, +0x1010 newspeed,
+0x1014 a, +0x1024 period, +0x1028 count, +0x102C n}; k4 bounce {+0x103C bspeed, +0x1050 n, +0x1054 max};
k5 wait timer (+0x105C); k6 wrap timer (+0x1088). Timers are the 0xC-byte Timer (05 §2).

### 5.3 Motion handlers (each frame while the bit is set)
- **0x1 speed-up**: if timer ≤ 16: vel = polar(angle, (5 − 5·t/16 + speed) × gs) (t = timer as float incl.
  sub-frame); else clear bit. Timer ticks.
- **0x10 accel**: if timer ≥ dur: clear. Else vel += avec × gs; if |vel.x| > 0.0001 or |vel.y| > 0.0001:
  angle = atan2(vel.y, vel.x). `speed` is not changed. Timer ticks.
- **0x20 angular**: if timer ≥ dur: clear. Else angle = wrap(angle + gs·dangle); speed += gs·dspeed;
  vel = polar(angle, gs·speed). Timer ticks.
- **0x40 / 0x100 / 0x80 redirect**: if timer < period: s = speed − t·speed/period (decelerates toward 0;
  `speed` itself unchanged). Else: sfx (if ≥ 0); n += 1, clear the bit when n ≥ count; new angle =
  angle + a (0x40, no wrap) / a (0x100) / wrap(angle_to_player(pos) + a) (0x80); speed = newspeed; s = speed;
  timer = 0. Every frame vel = polar(angle, s·gs); timer ticks.
- **0x400 / 0x800 bounce**: only when the bullet is off screen (§4.3 box test): sfx; if x < 0 or x ≥ 384:
  angle = wrap(−angle − π); if y < 0, or y ≥ 448 with bit 0x400: angle = −angle; speed = bspeed;
  vel = polar(angle, speed·gs); n += 1; when n ≥ max clear both 0x400 and 0x800. (0x800 does not reflect at the
  bottom but still counts/changes speed there.)
- **0x400000 wrap X**: if x < 0: x += 384, else if x > 384: x −= 384. Then if wrap timer ≤ 0 clear the bit,
  else timer.Decrement. **0x800000 wrap Y**: same with y and 448 (shares the timer).
- **0x20000 wait**: if timer ≤ 0 clear the bit, else timer.Decrement.

## 6. Lasers

### 6.1 Laser object (0x59C)
| off | type | meaning |
|---|---|---|
| +0 | AnmVm | body (script = laser type + 10, sprite += colour) |
| +0x2A4 | AnmVm | emitter glow (script 0x92 + table 0x4C70F8[colour]; VM flags bits 4–5 := 1) |
| +0x548 | f32[3] | origin |
| +0x554 | f32 | angle |
| +0x558 / +0x55C | f32 | tail / head distance from origin |
| +0x560 | f32 | max length |
| +0x564 | f32 | width |
| +0x568 | f32 | current width |
| +0x56C | f32 | speed (head growth per frame) |
| +0x570 | i32 | startup frames |
| +0x574 | i32 | collision start (frames into startup) |
| +0x578 | i32 | active duration |
| +0x57C | i32 | fade-out frames |
| +0x580 | i32 | collision frames during fade-out |
| +0x584 | i32 | alive |
| +0x588 | Timer | state timer |
| +0x594 | u16 | flags (bit 0: alpha fade instead of width; bit 2: immune to ordinary cancels and spawns during the cancel guard) |
| +0x596 | i16 | colour (reset to −1 by draw) |
| +0x598 | u8 | state 0 startup / 1 active / 2 fade |
| +0x599 | u8 | 0 |

### 6.2 Spawn (ECL ops 114/115 → 0x430F20; descriptor e+0x3070, 07 §9.4)
Only if the cancel guard is 0 or desc flags & 4. First free slot of 256 (none → no laser). origin = desc+4,
angle = desc+0x10 (+ angle to player when desc+0x1F8 == 0, i.e. op 115), speed = desc+0x18,
tail/head/maxlen/width = desc+0x1D0/0x1D4/0x1D8/0x1DC, startup/duration/fade = desc+0x1E0/0x1E4/0x1E8,
collision start = desc+0x1EC, fade collision = desc+0x1F0, flags = desc+0x1FC. State = 1 if startup == 0, else 0.
The laser pointer is stored in the enemy's laser slot (07 §9.4).

### 6.3 Per frame (alive lasers, slots 0..255)
1. head += gs·speed; if head − tail > maxlen: tail = head − maxlen; if tail < 0: tail = 0.
2. Collision rectangle in the laser frame (x along the laser): centre = (origin.x + tail + L/2, origin.y),
   size = (L × 0.7 if tail > 0 else L, width/2), L = head − tail. The player position is rotated by −angle about
   the origin; then box overlap with the player hitbox (half-extents player+0x3D4) using **half** the size above.
3. Body VM: scale_x = width / sprite width, scale_y = L / sprite height, rotation = wrap(angle + π/2).
4. State 0: if flag bit 0: VM alpha = min(255, trunc(t·255/startup)). Else current width = t·width/startup when
   t > startup − min(startup, 30), else 1.2; scale_x = current/16; **size.x := current/2** (original quirk: the
   along-axis extent is replaced, not the width). Test collision (no graze) when t ≥ collision start. When
   t ≥ startup: timer 0, state 1, current width = width, and state 1 runs this frame.
5. State 1: test with graze enabled only when t % 20 == 0; when t ≥ duration: timer 0, state 2; if fade == 0
   the laser dies, else state 2 runs this frame.
6. State 2: flag bit 0: alpha = min(255, trunc(t·255/**startup**)) (sic); else if fade > 0: current =
   width − t·width/fade, scale_x = current/16, size.x = current/2. Test collision (no graze) while
   t < fade-collision frames; die when t ≥ fade.
7. If still alive: die if tail ≥ 640; tick timer; run body VM.

Laser test result: hit (player state 0 → player death, 09 §7); otherwise when grazing is enabled and the
rectangle grown by 48 on each side overlaps and the player state is not 1/2 → graze (`Player_OnGraze` with the
laser flag, 09 §6).

Draw: body at origin + (cos, sin)·(tail + L/2) (cos/sin rounded to f32 first by `Math_SinCosF32` 0x433880, then f32 products — 05 §10 trig sites; the laser cancel positions of `BulletMgr_CancelAll`/`CancelBonus` use the same helper), z 0.06; emitter at origin + (cos, sin)·tail, z 0.05, drawn only
when (tail < 16 or speed == 0) and (+0x599 == 0 or state ≠ 0); emitter scale = (16 − tail)/16 × width/10
(≤ 0 → width/10), colour = body colour with alpha 255.

## 7. Cancels and conversions
- `BulletMgr_CancelAll(mode)` 0x430830 (mode 1 via 0x415C60 = ECL op 112, item collection etc.): every bullet
  not free and not in state 5: if inside a bomb area → item (player+0xE2A90, mode 1) and the slot is zeroed;
  else if mode == 4 → state 5; else item type 6 (mgr+0x6BA570) with item mode = `mode`, slot zeroed. Lasers
  (alive, and flag bit 2 clear unless mode 4): if state < 2: state 2, timer 0, width (+0x564) := current width (+0x568) so the fade shrinks from the current size; unless mode 4,
  one item per 32 px from tail to head along the laser. Fade-collision := 0. Cancel guard := 10.
- `BulletMgr_CancelBonus(maxValue, lasersToo)` 0x430AA0 (boss defeat, 07 §10.3): every non-free bullet →
  item (bomb-area item or type 6, mode 1), score popup value v starting at 2000, +20 per bullet, capped at
  maxValue (popup colour differs when capped); state 5. Returns the sum of popup values. Lasers: every alive
  laser — **no flag-bit-2 test** (unlike `CancelAll`, a flag-4 laser is cancelled too) — if state < 2: state 2,
  timer 0, width := current width; items only when lasersToo: first one at the origin, then one per 32 px from
  tail to head. Fade-collision := 0 for every alive laser. Cancel guard := 10 (VERIFIED 0x430AA0, RE-Q30; fullrun
  60112: five flag-4 lasers at a stage-6 card end give 50 extra type-6 items).
- `BulletMgr_CancelRadius(pos, r)` 0x430D30: bullets (not free/5) with |pos − p|² ≤ r² → item type 6 mode 1,
  slot zeroed.
- ECL natives 12 / 14 (0x424A20 / 0x424C40) — VERIFIED. Parameters come from the calling context: mask =
  var 10000 (ctx+0x18), mode = var 10001 (ctx+0x1C, native 12 only), angle = var 10016 (ctx+0x38), speed =
  var 10017 (ctx+0x3C). For every bullet slot with state ≠ 0 and (descriptor flags +0xDB0 & mask) ≠ 0, by
  its VM0 tag (+0x1FC, set by the bullet's ANM script):
  - Native 12: tag 1 → tag 0 "ghost": blend additive (flags bits 4–5 = 1), sprite index + 16, intangible
    (+0x10B4 = 1), velocity = polar(var 10016, gs·var 10017) (angle/speed fields unchanged). Any other tag →
    tag 1: blend alpha, sprite − 16, solid, velocity = polar(own angle +0xD74, gs·own speed +0xD68).
    Then if var 10001 == 0: every enemy reached through the e+8 link chain gets flags B bit 7 set and the two
    spell-background VMs (stage +0xB38, +0xDDC = globals 0x4E4B68 / 0x4E4E0C) get interrupt 2; else bit 7 is
    cleared and interrupt 1.
  - Native 14 (three-step cycle): tag 1 → tag 0 ghost as above but alpha byte := 0 and velocity = polar(own
    angle, gs·var 10017); tag 0 → tag 2: alpha 0 and an alpha interpolation 0 → 255 over 15 frames (mode 0,
    helper 0x40ED50) — the bullet fades back in, still intangible; tag 2 → tag 1: normal as native 12.

## 8. Items

### 8.1 Item manager (0x1653648)
2096 items × 0x2E4 at +0; dummy item at +0x17AAC0 (returned on failure); +0x17ADA4 search cursor;
+0x17ADA8 count (recomputed by calc); +0x17ADAC list sentinel (its next = +0x17B088 = first item),
+0x17B090 list tail. Live items form a doubly linked list in spawn order.

Item object: +0 AnmVm; +0x2A4 pos[3]; +0x2B0 vel[3] (mode 2: start pos); +0x2BC target[3]; +0x2C8 Timer;
+0x2D4 u8 type; +0x2D5 u8 alive; +0x2D6 u8 normal-sprite flag; +0x2D7 u8 state; +0x2D8 u8 max-value flag;
+0x2DC next; +0x2E0 prev.

### 8.2 Types (script = etama.anm 0x3D + type; arrow sprite 0xB6 + type)
| type | name | on collection |
|---|---|---|
| 0 | small power | power +1 (§8.5) |
| 1 | point | point value (§8.5) |
| 2 | big power | power +8 |
| 3 | bomb | bombs +1 if < 8 |
| 4 | full power | power := 128 |
| 5 | 1up | extend (§8.5) |
| 6 | cancel star | small score |
| 7 | time orb | §8.5 |
| 8 | converted point | point value / 10 |
| 10 | (spawn alias) | spawned as type 7 with mode 5 |

### 8.3 Spawn `ItemMgr_Spawn(pos, type, mode)`
- pos.x < −64 or > 448 → nothing (dummy).
- If power ≥ 128 and type is 0 or 2 → type 8. Type 7 → mode 3. Type 10 → type 7, mode 5.
- Search: cursor += 1 per probe; take the first free slot from the cursor (wrapping at 2096); **type 7 only
  probes one slot** (if occupied: nothing). On taking: if cursor > 2095 → 0.
- Init: pos; vel = (0, −2.2, 0); type; state = mode; timer 0.
  - mode 2: target = (Rng_Float·288 + 48, Rng_Float·192 − 64, 0) (x RNG first); vel field := start pos.
  - mode 3 / 5: vel.y = −2 − Rng_Float·0.2; vel.x = 0.6 · (Next32/2³¹ − 1) (RNG in that order); if the player
    state is 2: state 0, vel = (0, −0.9, 0).
- VM: script 0x3D + type, colour 0xFFFFFFFF, flag 0x2000; max-value flag 0; normal-sprite flag 1; appended.

Spawners: enemy drops (07 §10.4; mode 0, or 1 when the drop is "auto-collected"), random power/point extras
(offset Rng·128 − 64 on x then y; type 0 if power < 128 else 1; extra point items type 1; mode 0), bullet
cancels (type 6 / bomb-area type, mode 1), graze (type 10), timeline/ECL item ops.

### 8.4 Per frame (`ItemMgr_Calc`, list order)
Setup: collection box = (C, C) with C = sht field +0x18 (09 §4); fall multiplier m = sht +0x34 × gs, taken
from the youkai sht record when player byte +3 ≠ 0, else the human one. C, the attract speed (+0x14) and the
collection line (+0x1C) always come from the human sht (0x18B896C), whatever the focus byte.
For each item (count += 1):
- **state 2** (fly to target): if t < 60: pos = start·(1 − t/60) + target·t/60, then collection test. Else (at
  exactly t == 60: vel = 0, state 0); common motion.
- **state 3**: vel.y += 0.05·gs; if vel.y > 0 or the player's shot timer (0x18B89BC = player +0xE2AC4) is < 0, i.e. the player is not shooting → state 1; if player state is 2 →
  state 0, vel = (0, −0.7, 0). Common motion.
  Example (VERIFIED runtime, demo 2, changelog/answers_impl16.md §9): the shot timer reads 19 on frame 706 and −1 on 707 (player calc
  prio 9 runs before the item calc in `BulletMgr_Calc`, prio 14), so every time orb still rising in state 3 switches to
  state 1 on frame 707 even with vel.y ≈ −1.1, and orbs spawned later (not shooting) switch on their first update.
- **state 5**: vel.y += 0.05·gs; pos += vel·m; if vel.y > 0 → state 1 (player state 2 → state 0, vel (0,−0.7,0)),
  common motion; else no collection test this frame.
- **state 0/1**: attract when state == 1, or when player.y < the collection line (sht +0x1C) and (power ≥ 128, or
  player byte +3 ≠ 0, or the shot type 0x164D0B1 is 1 (Marisa & Alice) or 6 (Marisa solo)). Attract: if the player state is not 1/2: vel = polar(angle
  from item to player, sht +0x14), state 1, pos += vel·gs, then collection test (skip common motion); else
  vel.y = −0.7, state 0 (vel.x/z untouched: an item still above the line keeps drifting sideways every frame of
  the player's death, which is why `ItemMgr_DropAttracted` (09 §6.1) zeroes vel.x first; RE-Q28). Not attracting: vel.x = vel.z = 0; vel.y = max(vel.y, −2.2). Common motion.
- **common motion**: pos += vel·m. If state ≠ 0 or pos.y ≤ 448 + 16 (global 0x164D2E8 = 448): if vel.y ≥ 3 →
  vel.y = 3 else vel.y += 0.03·m; then collection test. Else (fell out): **rank −3** and the item is freed.
- **collection test**: skipped in state 3; `Player_ItemCollectTest` (player state 0/3/4; item box of size
  (C, C) vs the player's collect box player+0x3BC..+0x3CC). Both boxes are centred: item ±C/2, player ±collect/2 (09 §3, sht +0x18 / 2, recomputed
  in the player calc after movement), inclusive overlap of **f32 box edges**: item min = pos − C/2, max = pos + C/2 (C/2 via a ÷ 2.0), player min/max = the
  values stored by `Player_Move` (pos ∓ +0x3EC); not collected iff item max.x < player min.x or player max.x < item min.x
  (same for y). Each edge is rounded to f32 **before** the comparison, so this is not the same as |dx| ≤ (C + collect)/2:
  fullrun_noclip row 16164, item x 127.99999 + 12 rounds to 140.0 = player min.x → collected (VERIFIED 0x44A5A0, RE-Q29). On collection: statistics bit 0x40, type handler
  (§8.5), sound choice (0x15, or 0x2C if the item's max-value flag is set), item freed. Else tick timer and run
  the VM.
After the loop: play the chosen collection sound once; timer 0x18B89D4 (= player +0xE2ADC, "time-orb gauge
lock"): if > 0, decrement and clamp at 0. It is set to 50 by `Enemy_KillChildren` 0x42ADB0 when a chain is
killed by the player (together with `GameMgr_GaugeAdd(−gauge/12, not forced)` — skipped while bombing, 07 §10.3a — gauge timer B := 30, timer C := 0) and back to 0 when the
chain's time-orb shower is spawned.

### 8.5 Collection effects
Notation: `stats` = game-manager stats block (09 §9), score is stored /10 (`AddScore(v)` adds v/10).
- **Power (0: +1, 2: +8)**, only if power < 128: remember the power level (number of entries of the threshold
  table 0x4C7A6C `8, 24, 48, 80, 128, 999` that are ≤ power); power += n (`GameMgr_AddPower` 0x441850: checksum,
**4 RNG draws**); if power ≥ 128: power = 128 through `GameMgr_SetPower` 0x406FA0 (also checksummed: **+4 draws**,
so a collect that reaches full power costs 8; changelog/answers_impl18.md §IMPL19-Q1, VERIFIED code + runtime demo 3 frame 1059:
two collects = 4 + 4 + 4 = 12 draws), if no
  spell card is running cancel all bullets to items (mode 1), HUD message (`Hud_ShowMessage(0, 1)` "Full Power Mode!", 12 §5.5 — it also sets the panel full-repaint counter 0x17CE8CC := 2; VERIFIED 0x440CF0 / 0x441170, RE-Q33), convert all other type 0/2 items to
  type 8 (vel = (0, −0.5, 0) if vel.y > −0.5; effect; script 0x45). AddScore(10). Popup "10", or a power-up popup +
  sound 0x1F if the level changed. Type 0 additionally rank +1 (always, even at full power).
- **Point (1)**: max = stats+0x24. If item.y < collection line or the item's max-value flag: v = max; else
  v = max/2 − trunc(item.y − line)·(max/1000) (integer). v −= v % 10; if gauge ≥ threshold (0x406D10) v ×= 2.
  Popup (special colour when v ≥ max); max-value flag := 1 if v ≥ max; AddScore(v); stats+0x2C += 1,
  +0x30 += 1; rank +3 (+10 if v ≥ max); extends: while stats+0x34 ≥ 0 and +0x30 ≥ next (+0x38): extend
  (§8.5 1up rule), +0x34 += 1, next recomputed: normal difficulties (gm+0x30 < 4) n < 6 → table
  `100, 250, 500, 800, 1100, 9999`[n], else 9999 + 500·(n − 5); Extra (≥ 4): n < 3 → `200, 666, 9999`[n],
  else 99999. Counter 0x164CF9C += 1.
- **Converted point (8)**: like point but with max/10 and v/10 each rounded down to a multiple of 10, no counters,
  no rank.
- **Bomb (3)**: if bombs < 8: bombs += 1, HUD.
- **Full power (4)**: if power < 128: cancel to items (**unconditionally**, no spell-card test), HUD message 1 (`Hud_ShowMessage(0, 1)`, VERIFIED `ItemMgr_Calc` case 4, RE-Q33), sound 0x1F,
  popup, convert items. Always (even when already at 128): power := 128 via `GameMgr_SetPower` (checksum, 4 RNG
  draws), AddScore(1000), popup 1000.
- **1up (5) / extend**: if lives < 8: lives += 1, sound 0x1C, rank +200; else if bombs < 8: bombs += 1, sound
  0x1C, rank +200.
- **Star (6)**: v = 100 if player+0xE2A7C ≠ 0, else (stats+0xC / 40)·10 + 300 (< 1 → 10); popup + AddScore(v).
- **Time orb (7)**: v = 100 if player+0xE2A7C ≠ 0, else stats+0x2C < 2000 ? max(100, (stats+0x30 / 2)·10) :
  10000; popup; AddScore(v); game-manager time counter +1 (0x418220); spell bonus +8000 (0x416B10); if the
  timer 0x18B89D4 is 0: gauge += −111 (human) / +111 (youkai, player byte +3).

### 8.6 Item draw
At pos + (32, 16), z 0.15, `Draw2DRot`, in list order (VERIFIED `ItemMgr_Draw` 0x4415A0, RE-Q32). Item byte +0x2D6 =
"normal sprite shown" (set to 1 with colour 0xFFFFFFFF and z-write off by `ItemMgr_Spawn`). If pos.y < −8: drawn at
y = 8 + 16; if +0x2D6 = 1 → arrow sprite (0xB6 + type), +0x2D6 := 0, z-write off; then **every** such frame the
alpha byte := max(64, 255 − trunc((8 − y)·255/128)) (RGB kept). Else (y ≥ −8): only if +0x2D6 = 0 → normal sprite
(0xAC + type), +0x2D6 := 1, colour := 0xFFFFFFFF, z-write off. So an on-screen item is **not** reset each frame:
its ANM script's colour/alpha animation shows (time orb script 68: `color_time` 20 → (128, 128, 128) and
`alpha_time` 20 → 192 from t = 30), and returning from above the screen restores colour 0xFFFFFFFF once.

## 9. Open points
- Popup function 0x403200 / 0x403330 colour semantics and the effect manager are not specified here.
- Statistics word at (0x18B8A28)+0xDA (replay object, 15 §3): **write-only** (VERIFIED RE-Q26). `Replay_FrameHook` (prio 7)
  clears it every frame (bit 0x100 if 0x164D0A0 was set), events OR bits in (effect spawns 0x400, enemy spawn/free, items,
  player hit/graze/death/bomb, cancels …); no code reads it for a decision and `Replay_RecordCalc` stores only the input
  word. It can be ignored.
- Global 0x164D0B4 bit 10 (bullets frozen) belong to the game flow (11).
